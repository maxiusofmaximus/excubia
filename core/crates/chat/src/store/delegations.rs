//! Delegations and their cards. The tree is Alfred (depth 0) -> lead (1) ->
//! specialist (2); the specialist only answers and completes. The rules live
//! in [`super::delegation_rules`], the state machine in
//! [`super::delegation_state`] and the synthesis in [`super::synthesis`].

use rusqlite::{params, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model::preview;
use crate::model_team::{
    Delegation, DelegationDetail, DelegationState, NewDelegation, SynthesisInfo,
};
use crate::schema;

use super::delegation_rules::{
    active_for_agent, active_for_pane, assignment_title, bounded, card_for, card_id, check_tree,
    close_run, delegation_by_id, delegation_row, insert_card, message_by_id, recent_cycle,
    task_digest, update_card, NewCard,
};
use super::projects::seat_by_id;
use super::{append_event, conversation_exists};

pub const MAX_DEPTH: i64 = 2;
pub const TURN_BUDGET: i64 = 12;
pub const HOUR_BUDGET: i64 = 60;
pub const CYCLE_WINDOW_MINUTES: i64 = 10;
pub const TASK_CHARS: usize = 120;
pub const RESULT_CHARS: usize = 160;

pub(super) const DELEGATION_SELECT: &str =
    "SELECT d.id, d.conversation_id, d.project_id, d.parent_id,
            d.from_agent_id, f.name, d.to_agent_id, t.name,
            d.task, d.state, d.depth, d.turn_key, d.pane_id,
            d.result_message_id, d.reason, d.created_at, d.updated_at, d.finished_at
       FROM delegation d
       JOIN agent_seat f ON f.id = d.from_agent_id
       JOIN agent_seat t ON t.id = d.to_agent_id";

impl super::ChatStore {
    /// Validate and create one delegation, its run and its card atomically.
    pub fn create_delegation(&self, new: NewDelegation) -> Result<Delegation, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if !conversation_exists(&transaction, &new.conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        let from = seat_by_id(&transaction, &new.from_agent_id)?;
        let to = seat_by_id(&transaction, &new.to_agent_id)?;
        if from.agent_id == to.agent_id {
            return Err(ChatError::Cycle);
        }
        let (depth, project_id) = check_tree(&transaction, &from, &to, &new)?;
        let digest = task_digest(&new.task);
        if recent_cycle(&transaction, &from.agent_id, &to.agent_id, &digest)? {
            return Err(ChatError::Cycle);
        }
        let turn_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM delegation WHERE from_agent_id = ?1 AND turn_key = ?2",
            params![from.agent_id, new.turn_key],
            |row| row.get(0),
        )?;
        if turn_count >= TURN_BUDGET {
            return Err(ChatError::TurnBudget);
        }
        let hour_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM delegation
              WHERE from_agent_id = ?1
                AND created_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-60 minutes')",
            [&from.agent_id],
            |row| row.get(0),
        )?;
        if hour_count >= HOUR_BUDGET {
            return Err(ChatError::HourBudget);
        }
        if active_for_agent(&transaction, &to.agent_id)?.is_some() {
            return Err(ChatError::AssigneeBusy);
        }
        let now = schema::now(&transaction)?;
        let conversation_id = new.conversation_id.clone();
        let delegation_id = id::new_id("dlg");
        let run_id = id::new_id("run");
        let title = assignment_title(&from.name, &to.name);
        let task_summary = bounded(&new.task, TASK_CHARS);
        // The run comes first: the delegation row points at it.
        transaction.execute(
            "INSERT INTO run (id, agent_id, pane_id, message_id, state, started_at)
             VALUES (?1, ?2, ?3, NULL, 'queued', ?4)",
            params![run_id, to.agent_id, to.pane_id, now],
        )?;
        transaction.execute(
            "INSERT INTO delegation
                (id, company_id, project_id, conversation_id, parent_id,
                 from_agent_id, to_agent_id, task, digest, state, depth, turn_key,
                 run_id, pane_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'queued', ?10, ?11, ?12, ?13, ?14, ?14)",
            params![
                delegation_id,
                super::company_id(&transaction)?,
                project_id,
                conversation_id.clone(),
                new.parent_id,
                from.agent_id,
                to.agent_id,
                new.task,
                digest,
                depth,
                new.turn_key,
                run_id,
                to.pane_id,
                now
            ],
        )?;
        insert_card(
            &transaction,
            NewCard {
                conversation_id: &conversation_id,
                delegation_id: &delegation_id,
                title: &title,
                summary: &task_summary,
                state: DelegationState::Queued,
                seat: &to,
                now: &now,
            },
        )?;
        let frame = append_event(
            &transaction,
            "chat.delegation",
            &conversation_id,
            serde_json::json!({
                "event": "chat.delegation",
                "conversation_id": conversation_id,
                "delegation_id": delegation_id,
                "card_id": card_id(&delegation_id),
                "status": "queued",
                "from_agent_id": from.agent_id,
                "to_agent_id": to.agent_id,
                "pane_id": to.pane_id,
                "summary": task_summary,
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        drop(connection);
        self.delegation(&delegation_id)
    }

    pub fn delegation(&self, delegation_id: &str) -> Result<Delegation, ChatError> {
        let connection = self.lock();
        delegation_by_id(&connection, delegation_id)
    }

    /// The delegation the seat is executing right now. A lead uses it to scope
    /// its own assignments; a specialist to know what to complete.
    pub fn active_delegation_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Option<Delegation>, ChatError> {
        let connection = self.lock();
        active_for_agent(&connection, agent_id)
    }

    pub fn active_delegation_for_pane(
        &self,
        pane_id: &str,
    ) -> Result<Option<Delegation>, ChatError> {
        let connection = self.lock();
        active_for_pane(&connection, pane_id)
    }

    pub fn active_delegations_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<String>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id FROM delegation
              WHERE conversation_id = ?1 AND state IN ('queued', 'running', 'waiting')",
        )?;
        let rows = statement
            .query_map([conversation_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn delegation_detail(&self, delegation_id: &str) -> Result<DelegationDetail, ChatError> {
        let connection = self.lock();
        let delegation = delegation_by_id(&connection, delegation_id)?;
        let mut statement = connection.prepare(&format!(
            "{DELEGATION_SELECT} WHERE d.parent_id = ?1 ORDER BY d.created_at ASC, d.id ASC"
        ))?;
        let children = statement
            .query_map([delegation_id], delegation_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let card = card_for(&connection, delegation_id)?;
        let result_message = match delegation.result_message_id.as_deref() {
            Some(message_id) => message_by_id(&connection, message_id)?,
            None => None,
        };
        let synthesis = connection
            .query_row(
                "SELECT state, run_id FROM delegation_synthesis WHERE delegation_id = ?1",
                [delegation_id],
                |row| {
                    Ok(SynthesisInfo {
                        state: row.get(0)?,
                        run_id: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(DelegationDetail {
            delegation,
            children,
            card,
            result_message,
            synthesis,
        })
    }

    /// Dequeue a delegation: the pane accepted the task. Returns false when it
    /// was cancelled between creation and delivery.
    pub fn start_delegation(&self, delegation_id: &str) -> Result<bool, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let delegation = delegation_by_id(&transaction, delegation_id)?;
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE delegation SET state = 'running', updated_at = ?1
              WHERE id = ?2 AND state = 'queued'",
            params![now, delegation_id],
        )?;
        if changed == 1 {
            transaction.execute(
                "UPDATE timeline_card SET status = 'running', updated_at = ?1
                  WHERE delegation_id = ?2",
                params![now, delegation_id],
            )?;
            transaction.execute(
                "UPDATE run SET state = 'running', started_at = ?1
                  WHERE id = (SELECT run_id FROM delegation WHERE id = ?2)
                    AND state = 'queued'",
                params![now, delegation_id],
            )?;
            append_event(
                &transaction,
                "chat.activity",
                &delegation.conversation_id,
                serde_json::json!({
                    "event": "chat.activity",
                    "conversation_id": delegation.conversation_id,
                    "delegation_id": delegation_id,
                    "card_id": card_id(delegation_id),
                    "status": "running",
                    "pane_id": delegation.pane_id,
                    "summary": bounded(&delegation.task, TASK_CHARS),
                }),
            )?;
        }
        transaction.commit()?;
        Ok(changed == 1)
    }

    /// Finish a delegation with the assignee's result. The full text becomes an
    /// agent message (the card links to it); the card only gets one line.
    pub fn complete_delegation(
        &self,
        delegation_id: &str,
        result: &str,
    ) -> Result<String, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let delegation = delegation_by_id(&transaction, delegation_id)?;
        if !delegation.state.is_active() {
            return Err(ChatError::DelegationNotActive(delegation_id.to_string()));
        }
        let now = schema::now(&transaction)?;
        let (message_id, seq, _created_at) = self.insert_message(
            &transaction,
            &delegation.conversation_id,
            "agent",
            Some(&delegation.to_agent_id),
            result,
            "complete",
        )?;
        transaction.execute(
            "UPDATE delegation
                SET state = 'done', result_message_id = ?1, finished_at = ?2, updated_at = ?2,
                    reason = NULL
              WHERE id = ?3",
            params![message_id, now, delegation_id],
        )?;
        update_card(
            &transaction,
            delegation_id,
            &format!("{} terminó", delegation.to_name),
            &bounded(result, RESULT_CHARS),
            DelegationState::Done,
            0,
            &now,
        )?;
        close_run(&transaction, delegation_id, "completed")?;
        transaction.execute(
            "INSERT OR REPLACE INTO delegation_synthesis
                (delegation_id, conversation_id, result_message_id, state, run_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'pending', NULL, ?4, ?4)",
            params![delegation_id, delegation.conversation_id, message_id, now],
        )?;
        let created = append_event(
            &transaction,
            "chat.message",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": delegation.conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "created",
                "author_kind": "agent",
                "preview": preview(result),
            }),
        )?;
        let updated = append_event(
            &transaction,
            "chat.message",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": delegation.conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "updated",
                "author_kind": "agent",
                "state": "complete",
                "preview": preview(result),
            }),
        )?;
        let activity = append_event(
            &transaction,
            "chat.activity",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.activity",
                "conversation_id": delegation.conversation_id,
                "delegation_id": delegation_id,
                "card_id": card_id(delegation_id),
                "status": "done",
                "pane_id": delegation.pane_id,
                "summary": bounded(result, RESULT_CHARS),
            }),
        )?;
        transaction.commit()?;
        self.publish(created);
        self.publish(updated);
        self.publish(activity);
        Ok(message_id)
    }
}
