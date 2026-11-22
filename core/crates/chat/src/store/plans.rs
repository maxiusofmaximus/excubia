//! Plans (F3): the proposal, the operator's answer, and the gate a delegation of
//! more steps than the threshold passes before it may start. The rules are in
//! [`crate::model_plan`] and [`crate::policy::PlanRules`]; this file only keeps them.

use std::sync::PoisonError;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use crate::error::ChatError;
use crate::id;
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
        record_plan_line(&transaction, &plan)?;
        transaction.commit()?;
        Ok(plan)
    }

    /// The operator's answer. Only a proposed plan can be answered; the answer is final.
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
        if load(&transaction, plan_id)?.status != PlanStatus::Proposed {
            return Err(ChatError::PlanDecided);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE plan
                SET status = ?2, note = ?3, decided_by = ?4, decided_at = ?5, updated_at = ?5
              WHERE id = ?1",
            params![plan_id, decision.status().as_str(), note, decided_by, now],
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
