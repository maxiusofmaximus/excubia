//! The state machine of a delegation: failing, cancelling, the pane that
//! blocks or disappears, and the counter of the active ones.

use rusqlite::params;

use crate::error::ChatError;
use crate::model_team::{Delegation, DelegationState};
use crate::schema;

use super::delegation_rules::{
    active_delegations, active_for_pane, assignment_title, bounded, card_id, close_run,
    delegation_by_id, update_card,
};
use super::delegations::TASK_CHARS;
use super::{append_event, conversation_exists};

impl super::ChatStore {
    pub fn fail_delegation(&self, delegation_id: &str, reason: &str) -> Result<bool, ChatError> {
        self.finish_delegation(delegation_id, DelegationState::Failed, reason)
    }

    pub fn cancel_delegation(&self, delegation_id: &str) -> Result<Delegation, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let delegation = delegation_by_id(&transaction, delegation_id)?;
        if delegation.state.is_terminal() {
            return Err(ChatError::DelegationNotActive(delegation_id.to_string()));
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE delegation SET state = 'cancelled', finished_at = ?1, updated_at = ?1,
                    reason = 'delegation_cancelled'
              WHERE id = ?2 AND state IN ('queued', 'running', 'waiting')",
            params![now, delegation_id],
        )?;
        update_card(
            &transaction,
            delegation_id,
            &format!("{} cancelado", delegation.to_name),
            &bounded(&delegation.task, TASK_CHARS),
            DelegationState::Cancelled,
            0,
            &now,
        )?;
        close_run(&transaction, delegation_id, "interrupted")?;
        let activity = append_event(
            &transaction,
            "chat.activity",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.activity",
                "conversation_id": delegation.conversation_id,
                "delegation_id": delegation_id,
                "card_id": card_id(delegation_id),
                "status": "cancelled",
                "pane_id": delegation.pane_id,
                "summary": bounded(&delegation.task, TASK_CHARS),
            }),
        )?;
        transaction.commit()?;
        self.publish(activity);
        drop(connection);
        self.delegation(delegation_id)
    }

    /// Kill switch: cancel every active delegation of one conversation. Returns
    /// the cancelled delegations so the caller can interrupt their panes.
    pub fn cancel_conversation(&self, conversation_id: &str) -> Result<Vec<Delegation>, ChatError> {
        let connection = self.lock();
        if !conversation_exists(&connection, conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        let ids: Vec<String> = {
            let mut statement = connection.prepare(
                "SELECT id FROM delegation
                  WHERE conversation_id = ?1 AND state IN ('queued', 'running', 'waiting')",
            )?;
            let rows = statement
                .query_map([conversation_id], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            rows
        };
        let now = schema::now(&connection)?;
        connection.execute(
            "UPDATE run SET state = 'interrupted', finished_at = ?1, reason = 'conversation_cancelled'
              WHERE state IN ('queued', 'running') AND message_id IN (
                  SELECT id FROM message WHERE conversation_id = ?2)",
            params![now, conversation_id],
        )?;
        drop(connection);
        let mut cancelled = Vec::new();
        for id in ids {
            if let Ok(delegation) = self.cancel_delegation(&id) {
                cancelled.push(delegation);
            }
        }
        Ok(cancelled)
    }

    /// `pane.*` status: a working pane is running, a blocked one is waiting.
    pub fn delegation_state_from_pane(
        &self,
        pane_id: &str,
        state: DelegationState,
    ) -> Result<bool, ChatError> {
        if !matches!(state, DelegationState::Running | DelegationState::Waiting) {
            return Err(ChatError::Inconsistent(
                "pane state is not a delegation state",
            ));
        }
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let Some(delegation) = active_for_pane(&transaction, pane_id)? else {
            return Ok(false);
        };
        if delegation.state == state {
            return Ok(false);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE delegation SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![state.as_str(), now, delegation.id],
        )?;
        if state == DelegationState::Running && delegation.state == DelegationState::Queued {
            transaction.execute(
                "UPDATE run SET state = 'running', started_at = ?1
                  WHERE id = (SELECT run_id FROM delegation WHERE id = ?2)
                    AND state = 'queued'",
                params![now, delegation.id],
            )?;
        }
        let (title, attention) = match state {
            DelegationState::Waiting => (format!("{} necesita permiso", delegation.to_name), 3),
            _ => (
                assignment_title(&delegation.from_name, &delegation.to_name),
                0,
            ),
        };
        update_card(
            &transaction,
            &delegation.id,
            &title,
            &bounded(&delegation.task, TASK_CHARS),
            state,
            attention,
            &now,
        )?;
        let activity = append_event(
            &transaction,
            "chat.activity",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.activity",
                "conversation_id": delegation.conversation_id,
                "delegation_id": delegation.id,
                "card_id": card_id(&delegation.id),
                "status": state.as_str(),
                "pane_id": delegation.pane_id,
                "summary": bounded(&delegation.task, TASK_CHARS),
            }),
        )?;
        transaction.commit()?;
        self.publish(activity);
        Ok(true)
    }

    /// The pane disappeared while the delegation was active: keep the result
    /// (if any) but stop pointing at a dead panel.
    pub fn fail_delegation_for_pane(&self, pane_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let Some(delegation) = active_for_pane(&connection, pane_id)? else {
            return Ok(false);
        };
        connection.execute(
            "UPDATE delegation SET pane_id = NULL WHERE id = ?1",
            [&delegation.id],
        )?;
        connection.execute(
            "UPDATE timeline_card SET pane_id = NULL WHERE delegation_id = ?1",
            [&delegation.id],
        )?;
        drop(connection);
        self.finish_delegation(&delegation.id, DelegationState::Failed, "pane_gone")
    }

    /// How many delegations are queued, running or waiting. This is the
    /// `active_delegations` counter of `/v1/state` and the bootstrap.
    pub fn active_delegation_count(&self) -> Result<u64, ChatError> {
        let connection = self.lock();
        active_delegations(&connection)
    }

    fn finish_delegation(
        &self,
        delegation_id: &str,
        state: DelegationState,
        reason: &str,
    ) -> Result<bool, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let delegation = delegation_by_id(&transaction, delegation_id)?;
        if !delegation.state.is_active() {
            return Ok(false);
        }
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE delegation SET state = ?1, reason = ?2, finished_at = ?3, updated_at = ?3
              WHERE id = ?4 AND state IN ('queued', 'running', 'waiting')",
            params![state.as_str(), reason, now, delegation_id],
        )?;
        if changed == 0 {
            return Ok(false);
        }
        update_card(
            &transaction,
            delegation_id,
            &format!("{} no terminó", delegation.to_name),
            &bounded(reason, TASK_CHARS),
            state,
            0,
            &now,
        )?;
        close_run(&transaction, delegation_id, state.as_str())?;
        let activity = append_event(
            &transaction,
            "chat.activity",
            &delegation.conversation_id,
            serde_json::json!({
                "event": "chat.activity",
                "conversation_id": delegation.conversation_id,
                "delegation_id": delegation_id,
                "card_id": card_id(delegation_id),
                "status": state.as_str(),
                "pane_id": delegation.pane_id,
                "summary": bounded(reason, TASK_CHARS),
            }),
        )?;
        transaction.commit()?;
        self.publish(activity);
        Ok(true)
    }
}
