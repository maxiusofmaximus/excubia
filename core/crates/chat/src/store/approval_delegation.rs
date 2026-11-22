//! Approvals decided by an agent instead of the operator. A lead decides the
//! requests of the seats of its own project; the CTO (Alfred) decides the
//! requests at its level. The host reads who decides from the seat that calls,
//! and where the approval sits from its row: the body only carries the decision.

use rusqlite::{params, Transaction};

use crate::clock::iso;
use crate::error::ChatError;
use crate::model_approval::{Approval, ApprovalStatus, Decision};
use crate::policy::{Level, Reason};
use crate::seat::SeatActor;

use super::approval_rules::{
    approval_by_id, approval_from_columns, approval_row, audit_of, frame_of, project_of_seat,
    write_audit, Decider,
};
use super::approval_settle;
use super::approvals::APPROVAL_SELECT;

/// How many decisions one agent may make in an hour, whatever they decide.
pub const DELEGATED_DECISIONS_PER_HOUR: i64 = 6;

const HOUR_MS: i64 = 3_600_000;

const LEAD_LEVEL: &str = "lead";
const CTO_LEVEL: &str = "cto";

/// Who decides delegated approvals. Every other seat decides none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Approver<'a> {
    Lead { project_id: &'a str },
    Cto,
}

impl<'a> Approver<'a> {
    fn of(actor: &'a SeatActor) -> Result<Self, ChatError> {
        match (actor.role_id.as_str(), actor.project_id.as_deref()) {
            ("alfred", _) => Ok(Self::Cto),
            ("lead", Some(project_id)) => Ok(Self::Lead { project_id }),
            _ => Err(ChatError::ApprovalScopeDenied),
        }
    }

    /// The level an approval must sit at for this approver to decide it.
    fn level(self) -> &'static str {
        match self {
            Self::Lead { .. } => LEAD_LEVEL,
            Self::Cto => CTO_LEVEL,
        }
    }
}

impl super::ChatStore {
    /// A seat's decision on an approval waiting at its level. The decision is
    /// one transaction: the checks, the write and its audit row.
    pub fn decide_delegated(
        &self,
        actor: &SeatActor,
        approval_id: &str,
        decision: Decision,
        reason: Option<&str>,
    ) -> Result<Approval, ChatError> {
        self.expire_now()?;
        let approver = Approver::of(actor)?;
        let now_ms = self.now_ms();
        let now = iso(now_ms);
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let approval = approval_by_id(&transaction, approval_id)?;
        let hour_ago = iso(now_ms - HOUR_MS);
        check_delegation(&transaction, actor, approver, &approval, &hour_ago)?;
        transaction.execute(
            "UPDATE approval SET status = ?1, decided_by = ?2, decided_at = ?3, updated_at = ?3,
                    decider_kind = 'agent', decider_id = ?2
              WHERE id = ?4 AND status = 'pending'",
            params![decision.as_str(), actor.agent_id, now, approval_id],
        )?;
        let decider = Decider {
            kind: "agent",
            id: &actor.agent_id,
            level: Some(approver.level()),
            reason,
            scorer_version: None,
        };
        write_audit(
            &transaction,
            approval_id,
            &approval.action_digest,
            &decider,
            decision.as_str(),
            &now,
        )?;
        let decided = approval_by_id(&transaction, approval_id)?;
        approval_settle::record_timeline_line(&transaction, &decided)?;
        let frame = frame_of(&transaction, approval_id, "decided")?;
        transaction.commit()?;
        self.publish(frame);
        drop(connection);
        self.approval(approval_id)
    }

    /// The approvals waiting at the level of this seat: its project's for a
    /// lead, the company's for Alfred. The seat's own requests are never listed.
    pub fn pending_for_approver(&self, actor: &SeatActor) -> Result<Vec<Approval>, ChatError> {
        self.expire_now()?;
        let approver = Approver::of(actor)?;
        let project_id = match approver {
            Approver::Lead { project_id } => Some(project_id),
            Approver::Cto => None,
        };
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "{APPROVAL_SELECT}
              WHERE status = 'pending' AND level = ?1
                AND (?2 IS NULL OR seat_id IN (SELECT id FROM agent_seat WHERE project_id = ?2))
                AND (seat_id IS NULL OR seat_id != ?3)
              ORDER BY created_at, id"
        ))?;
        let rows = statement.query_map(
            params![approver.level(), project_id, actor.agent_id],
            approval_row,
        )?;
        let mut approvals = Vec::new();
        for row in rows {
            let mut approval = approval_from_columns(row?)?;
            approval.audit = audit_of(&connection, &approval.id)?;
            approvals.push(approval);
        }
        Ok(approvals)
    }
}

/// The rules of a delegated decision, in order. The first one that fails names
/// the refusal; nothing is written unless all of them pass.
fn check_delegation(
    transaction: &Transaction<'_>,
    actor: &SeatActor,
    approver: Approver<'_>,
    approval: &Approval,
    hour_ago: &str,
) -> Result<(), ChatError> {
    match approval.status {
        ApprovalStatus::Pending => {}
        ApprovalStatus::Expired => return Err(ChatError::ApprovalExpired),
        ApprovalStatus::Approved | ApprovalStatus::Denied | ApprovalStatus::Consumed => {
            return Err(ChatError::ApprovalAlreadyDecided)
        }
    }
    let user_only = approval.level.as_deref() == Some(Level::User.as_str())
        || approval.reason.as_deref() == Some(Reason::NeverDelegate.as_str());
    if user_only {
        return Err(ChatError::ApprovalNotDelegable);
    }
    if is_requester(actor, approval) {
        return Err(ChatError::ApprovalSelfApproval);
    }
    if approval.level.as_deref() != Some(approver.level()) {
        return Err(ChatError::ApprovalLevelMismatch);
    }
    if let Approver::Lead { project_id } = approver {
        let requester = match approval.seat_id.as_deref() {
            Some(seat_id) => project_of_seat(transaction, seat_id)?,
            None => None,
        };
        if requester.as_deref() != Some(project_id) {
            return Err(ChatError::ApprovalScopeDenied);
        }
    }
    if decisions_this_hour(transaction, &actor.agent_id, hour_ago)? >= DELEGATED_DECISIONS_PER_HOUR
    {
        return Err(ChatError::ApprovalBudget);
    }
    Ok(())
}

/// The seat that asked, or the run that asked. A resident seat has no run id,
/// so an empty one never matches.
fn is_requester(actor: &SeatActor, approval: &Approval) -> bool {
    approval.seat_id.as_deref() == Some(actor.agent_id.as_str())
        || (!actor.run_id.is_empty() && approval.run_id.as_deref() == Some(actor.run_id.as_str()))
}

/// The decisions this agent made since `since`, on the store's clock: the
/// budget is an hour long, measured in the same text the rows store.
fn decisions_this_hour(
    transaction: &Transaction<'_>,
    agent_id: &str,
    since: &str,
) -> Result<i64, ChatError> {
    Ok(transaction.query_row(
        "SELECT COUNT(*) FROM approval_audit
          WHERE decider_kind = 'agent' AND decider_id = ?1
            AND decision IN ('approved', 'denied')
            AND decided_at > ?2",
        params![agent_id, since],
        |row| row.get(0),
    )?)
}
