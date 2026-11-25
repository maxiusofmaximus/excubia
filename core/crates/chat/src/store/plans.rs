//! Plans (F3): the proposal, the operator's answer, and the gate a delegation of
//! more steps than the threshold passes before it may start. The rules are in
//! [`crate::model_plan`] and [`crate::policy::PlanRules`]; this file only keeps them.

use std::sync::PoisonError;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::error::ChatError;
use crate::id;
use crate::model::{RunState, Turn};
use crate::model_mode::ApprovalMode;
use crate::model_plan::{
    self, summary, touches_of, NewPlan, Plan, PlanDecision, PlanStatus, PlanStep,
};
use crate::policy::PlanRules;
use crate::schema;

use super::approval_mode;
use super::approval_timeline::record_plan_line;
use super::conversation_exists;
use super::projects::seat_by_id;

/// The most plans one list call returns.
const LIST_LIMIT: u32 = 200;

const PLAN_SELECT: &str = "SELECT p.id, p.conversation_id, p.proposer_agent_id, s.name,
            p.steps, p.status, p.note, p.delegation_id, p.decided_by,
            p.created_at, p.updated_at, p.decided_at
       FROM plan p
       JOIN agent_seat s ON s.id = p.proposer_agent_id";

impl super::ChatStore {
    /// The plan rules of the policy file as they stand now.
    pub(super) fn plan_rules(&self) -> PlanRules {
        let mut policy = self.policy.lock().unwrap_or_else(PoisonError::into_inner);
        policy.current().plans().clone()
    }

    /// Store one proposed plan. It is approved by rule only when the policy
    /// allows it, nothing it touches reaches out, and the mode is `delegated`.
    pub fn propose_plan(&self, new: NewPlan) -> Result<Plan, ChatError> {
        self.propose_plan_with(new, false)
    }

    /// As [`Self::propose_plan`]. With `run_on_approval` the operator's approval hands the
    /// plan to Alfred to start it (see [`Self::claim_plan_resume`]). A plan the rule
    /// approves is never handed over: its agent already reads it as approved.
    pub fn propose_plan_with(
        &self,
        new: NewPlan,
        run_on_approval: bool,
    ) -> Result<Plan, ChatError> {
        model_plan::validate(&new.steps)?;
        let rules = self.plan_rules();
        let touches = touches_of(&new.steps);
        let steps_json = serde_json::to_string(&new.steps)
            .map_err(|_| ChatError::InvalidPlan("the steps cannot be stored"))?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if !conversation_exists(&transaction, &new.conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        seat_by_id(&transaction, &new.proposer_agent_id)?;
        let project: Option<String> = transaction.query_row(
            "SELECT project_id FROM conversation WHERE id = ?1",
            [&new.conversation_id],
            |row| row.get(0),
        )?;
        let by_rule = rules.may_auto_approve(&touches)
            && approval_mode::effective(&transaction, project.as_deref())?
                == ApprovalMode::Delegated;
        let now = schema::now(&transaction)?;
        let plan_id = id::new_id("plan");
        let (status, decided_by, decided_at) = if by_rule {
            (PlanStatus::Approved, Some("policy"), Some(now.clone()))
        } else {
            (PlanStatus::Proposed, None, None)
        };
        transaction.execute(
            "INSERT INTO plan
                (id, conversation_id, proposer_agent_id, steps, status, decided_by,
                 created_at, updated_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
            params![
                plan_id,
                new.conversation_id,
                new.proposer_agent_id,
                steps_json,
                status.as_str(),
                decided_by,
                now,
                decided_at,
            ],
        )?;
        let plan = load(&transaction, &plan_id)?;
        if run_on_approval && status == PlanStatus::Proposed {
            transaction.execute(
                "INSERT INTO plan_resume
                    (plan_id, conversation_id, state, run_id, created_at, updated_at)
                 VALUES (?1, ?2, 'armed', NULL, ?3, ?3)",
                params![plan_id, new.conversation_id, now],
            )?;
        }
        record_plan_line(&transaction, &plan)?;
        transaction.commit()?;
        Ok(plan)
    }

    /// The operator's answer. Only a proposed plan can be answered; the answer is final.
    /// A repeated approval of a plan that was armed to run returns the plan as it stands:
    /// the answer is the same, and nothing is written again.
    pub fn decide_plan(
        &self,
        plan_id: &str,
        decision: PlanDecision,
        note: Option<String>,
        decided_by: &str,
    ) -> Result<Plan, ChatError> {
        if let Some(note) = &note {
            model_plan::validate_note(note)?;
        }
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = load(&transaction, plan_id)?;
        if current.status != PlanStatus::Proposed {
            let armed: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM plan_resume WHERE plan_id = ?1",
                [plan_id],
                |row| row.get(0),
            )?;
            if decision == PlanDecision::Approve && current.status == PlanStatus::Approved && armed > 0
            {
                return Ok(current);
            }
            return Err(ChatError::PlanDecided);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE plan
                SET status = ?2, note = ?3, decided_by = ?4, decided_at = ?5, updated_at = ?5
              WHERE id = ?1",
            params![plan_id, decision.status().as_str(), note, decided_by, now],
        )?;
        // Approval hands an armed plan over; any other answer closes it for good.
        let resume = if decision == PlanDecision::Approve {
            "pending"
        } else {
            "skipped"
        };
        transaction.execute(
            "UPDATE plan_resume SET state = ?2, updated_at = ?3
              WHERE plan_id = ?1 AND state = 'armed'",
            params![plan_id, resume, now],
        )?;
        let plan = load(&transaction, plan_id)?;
        record_plan_line(&transaction, &plan)?;
        transaction.commit()?;
        Ok(plan)
    }

    /// One plan, or `NoSuchPlan`.
    pub fn plan(&self, plan_id: &str) -> Result<Plan, ChatError> {
        load(&self.lock(), plan_id)
    }

    /// The plans, newest first, optionally only those in one status.
    pub fn plans(&self, status: Option<PlanStatus>) -> Result<Vec<Plan>, ChatError> {
        let connection = self.lock();
        let sql = format!(
            "{PLAN_SELECT} WHERE (?1 IS NULL OR p.status = ?1)
              ORDER BY p.created_at DESC, p.id DESC LIMIT {LIST_LIMIT}"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([status.map(PlanStatus::as_str)], plan_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The plans approved to run and not yet handed to Alfred, oldest first.
    pub fn pending_plan_resumes(&self) -> Result<Vec<String>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT plan_id FROM plan_resume WHERE state = 'pending' ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
    }

    /// Hand an approved plan to Alfred as one turn, queued in the same transaction that
    /// marks it `queued`. The turn is a system text: no user message is written. `None`
    /// when there is nothing to hand over (not pending, already handed over, linked to a
    /// delegation, or its conversation is archived). The caller dispatches the turn and
    /// calls [`Self::release_plan_resume`] when the seat refuses it.
    pub fn claim_plan_resume(&self, plan_id: &str) -> Result<Option<Turn>, ChatError> {
        let alfred_id = self.alfred_id()?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let state: Option<String> = transaction
            .query_row(
                "SELECT state FROM plan_resume WHERE plan_id = ?1",
                [plan_id],
                |row| row.get(0),
            )
            .optional()?;
        if state.as_deref() != Some("pending") {
            return Ok(None);
        }
        let plan = load(&transaction, plan_id)?;
        let archived: i64 = transaction.query_row(
            "SELECT archived FROM conversation WHERE id = ?1",
            [&plan.conversation_id],
            |row| row.get(0),
        )?;
        let now = schema::now(&transaction)?;
        // A plan a delegation already started from, or one in an archived conversation, is
        // not run again: the resumption closes without a turn.
        if plan.delegation_id.is_some() || archived != 0 || plan.status != PlanStatus::Approved {
            transaction.execute(
                "UPDATE plan_resume SET state = 'skipped', updated_at = ?2 WHERE plan_id = ?1",
                params![plan_id, now],
            )?;
            transaction.commit()?;
            return Ok(None);
        }
        let destination = resume_destination(&transaction, &plan, &alfred_id)?;
        let run_id = id::new_id("run");
        transaction.execute(
            "INSERT INTO run (id, agent_id, pane_id, message_id, state, started_at)
             VALUES (?1, ?2, NULL, NULL, 'queued', ?3)",
            params![run_id, alfred_id, now],
        )?;
        transaction.execute(
            "UPDATE plan_resume SET state = 'queued', run_id = ?2, updated_at = ?3
              WHERE plan_id = ?1",
            params![plan_id, run_id, now],
        )?;
        transaction.commit()?;
        Ok(Some(Turn {
            id: run_id,
            state: RunState::Queued,
            agent_id: alfred_id,
            conversation_id: plan.conversation_id.clone(),
            message_id: String::new(),
            text: model_plan::resume_text(&plan, &destination),
        }))
    }

    /// The seat refused the turn [`Self::claim_plan_resume`] returned. In one transaction the
    /// reserved run is closed as failed and the plan goes back to pending, so the next
    /// approval answer or the start of the host hands it over once more, on a new run.
    pub fn release_plan_resume(&self, plan_id: &str) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        let reserved: Option<Option<String>> = transaction
            .query_row(
                "SELECT run_id FROM plan_resume WHERE plan_id = ?1 AND state = 'queued'",
                [plan_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(run_id) = reserved.flatten() {
            transaction.execute(
                "UPDATE run SET state = 'failed', finished_at = ?2, reason = 'seat_unavailable'
                  WHERE id = ?1 AND state = 'queued'",
                params![run_id, now],
            )?;
            transaction.execute(
                "UPDATE plan_resume SET state = 'pending', run_id = NULL, updated_at = ?2
                  WHERE plan_id = ?1",
                params![plan_id, now],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Whether a plan is one that the approval hands to Alfred (`run_on_approval`). Recipes
    /// and schedules never start such a plan themselves.
    pub fn is_run_on_approval(&self, plan_id: &str) -> Result<bool, ChatError> {
        is_run_plan(&self.lock(), plan_id)
    }
}

/// Closes the live plan runs of a conversation (stop, archive): their runs become `interrupted`
/// and their resumptions `skipped`, so a queued hand-over never reaches the seat as work.
pub(super) fn interrupt_plan_runs(
    connection: &Connection,
    conversation_id: &str,
    reason: &str,
) -> Result<(), ChatError> {
    let now = schema::now(connection)?;
    connection.execute(
        "UPDATE run SET state = 'interrupted', finished_at = ?2, reason = ?3
          WHERE state IN ('queued', 'running') AND id IN (
              SELECT run_id FROM plan_resume WHERE conversation_id = ?1 AND run_id IS NOT NULL)",
        params![conversation_id, now, reason],
    )?;
    connection.execute(
        "UPDATE plan_resume SET state = 'skipped', updated_at = ?2
          WHERE conversation_id = ?1 AND state = 'queued'",
        params![conversation_id, now],
    )?;
    Ok(())
}

/// Whether the plan has a resumption row, i.e. it was proposed with `run_on_approval`.
pub(super) fn is_run_plan(connection: &Connection, plan_id: &str) -> Result<bool, ChatError> {
    let rows: i64 = connection.query_row(
        "SELECT COUNT(*) FROM plan_resume WHERE plan_id = ?1",
        [plan_id],
        |row| row.get(0),
    )?;
    Ok(rows > 0)
}

/// Who Alfred sends an approved plan to: the lead that proposed it, or else the lead of the
/// conversation's project. The text names the seat by id, so the turn needs no lookup.
fn resume_destination(
    connection: &Connection,
    plan: &Plan,
    alfred_id: &str,
) -> Result<String, ChatError> {
    if plan.proposer_agent_id != alfred_id {
        return Ok(format!(
            "el líder {} (to_agent_id \"{}\")",
            plan.proposer_name, plan.proposer_agent_id
        ));
    }
    let lead: Option<(String, String)> = connection
        .query_row(
            "SELECT s.id, s.name FROM agent_seat s
               JOIN conversation c ON c.project_id = s.project_id
              WHERE c.id = ?1 AND s.role_id = 'lead'
              LIMIT 1",
            [&plan.conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(match lead {
        Some((id, name)) => format!("el líder del proyecto {name} (to_agent_id \"{id}\")"),
        None => "el líder del proyecto".to_string(),
    })
}

/// The plan a delegation of `steps` steps starts under, or `None` when it needs
/// none. A plan must be approved, unlinked and in the same conversation; the
/// caller links it with [`link_plan`] once the delegation row exists. With a
/// plan, the plan's own steps stand and the declared count is not read.
pub(super) fn gate(
    connection: &Connection,
    rules: &PlanRules,
    conversation_id: &str,
    steps: u32,
    plan_id: Option<&str>,
) -> Result<Option<String>, ChatError> {
    let Some(plan_id) = plan_id else {
        return if rules.requires_plan(steps) {
            Err(ChatError::PlanRequired)
        } else {
            Ok(None)
        };
    };
    let plan = load(connection, plan_id)?;
    if plan.delegation_id.is_some() {
        return Err(ChatError::PlanLinked);
    }
    if plan.status != PlanStatus::Approved || plan.conversation_id != conversation_id {
        return Err(ChatError::PlanNotApproved);
    }
    Ok(Some(plan.id))
}

pub(super) fn link_plan(
    transaction: &Transaction<'_>,
    plan_id: &str,
    delegation_id: &str,
) -> Result<(), ChatError> {
    transaction.execute(
        "UPDATE plan SET delegation_id = ?2 WHERE id = ?1",
        params![plan_id, delegation_id],
    )?;
    Ok(())
}

pub(super) fn load(connection: &Connection, plan_id: &str) -> Result<Plan, ChatError> {
    let sql = format!("{PLAN_SELECT} WHERE p.id = ?1");
    connection
        .query_row(&sql, [plan_id], plan_from_row)
        .optional()?
        .ok_or_else(|| ChatError::NoSuchPlan(plan_id.to_string()))
}

fn plan_from_row(row: &Row<'_>) -> rusqlite::Result<Plan> {
    let steps_json: String = row.get(4)?;
    let steps: Vec<PlanStep> = serde_json::from_str(&steps_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(error))
    })?;
    let status_text: String = row.get(5)?;
    let status = PlanStatus::from_db(&status_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(5, Type::Text, "unknown plan status".into())
    })?;
    let proposer_name: String = row.get(3)?;
    Ok(Plan {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        proposer_agent_id: row.get(2)?,
        summary: summary(&steps, &proposer_name),
        proposer_name,
        steps,
        status,
        note: row.get(6)?,
        delegation_id: row.get(7)?,
        decided_by: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        decided_at: row.get(11)?,
    })
}
