//! The decisions that end a pending approval and the consumption that burns an
//! approved one: the operator's answer, the agent's hook, and the host itself
//! for the approvals it opened. Each one is one transaction with its audit row.

use rusqlite::{params, Connection};

use crate::error::ChatError;
use crate::model_approval::{Approval, ApprovalStatus, Decision, HOOK_CONSUMER};
use crate::model_hook::HookScope;

use super::approval_rules::{approval_by_id, frame_of, write_audit, Decider};
use super::approval_settle;

/// Who burns an approval. The host may burn what its own MCP tool opened; the
/// agent's hook only burns what its token covers.
#[derive(Clone, Copy)]
pub(super) enum Burn<'a> {
    Host,
    Hook(&'a HookScope),
}

/// Whether a decision with this `client_id` already answered the approval.
fn answered_by(
    connection: &Connection,
    approval_id: &str,
    client_id: &str,
) -> Result<bool, ChatError> {
    Ok(connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM approval_audit WHERE approval_id = ?1 AND client_id = ?2)",
        params![approval_id, client_id],
        |row| row.get(0),
    )?)
}

impl super::ChatStore {
    /// The operator's answer. `device_id` is the signed device that decides: it
    /// is the decider of the audit, as in the mode settings. `client_id` is only
    /// the idempotency key: the same one always gets the same answer back, and
    /// anything else over a resolved approval is a conflict.
    pub fn decide_approval(
        &self,
        approval_id: &str,
        decision: Decision,
        client_id: &str,
        device_id: &str,
    ) -> Result<Approval, ChatError> {
        self.expire_now()?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = self.now_text();
        let approval = approval_by_id(&transaction, approval_id)?;
        match approval.status {
            ApprovalStatus::Pending => {}
            ApprovalStatus::Approved | ApprovalStatus::Denied => {
                if answered_by(&transaction, approval_id, client_id)? {
                    transaction.commit()?;
                    drop(connection);
                    return self.approval(approval_id);
                }
                return Err(ChatError::ApprovalAlreadyDecided);
            }
            ApprovalStatus::Expired => return Err(ChatError::ApprovalExpired),
            ApprovalStatus::Consumed => return Err(ChatError::ApprovalAlreadyDecided),
        }
        transaction.execute(
            "UPDATE approval SET status = ?1, decided_by = ?2, decided_at = ?3, updated_at = ?3,
                    decider_kind = 'user', decider_id = ?2
              WHERE id = ?4 AND status = 'pending'",
            params![decision.as_str(), device_id, now, approval_id],
        )?;
        let decider = Decider {
            kind: "user",
            id: device_id,
            level: None,
            reason: None,
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
        transaction.execute(
            "UPDATE approval_audit SET client_id = ?1 WHERE rowid = ?2",
            params![client_id, transaction.last_insert_rowid()],
        )?;
        let decided_row = approval_by_id(&transaction, approval_id)?;
        approval_settle::record_timeline_line(&transaction, &decided_row)?;
        let frame = frame_of(&transaction, approval_id, "decided")?;
        transaction.commit()?;
        self.publish(frame);
        drop(connection);
        self.approval(approval_id)
    }

    /// Burn the approval just before the action runs. It happens once: a
    /// second attempt is a conflict, never a second execution. Only the hook
    /// token of the run (or panel) the approval belongs to may burn it.
    pub fn consume_approval(
        &self,
        approval_id: &str,
        digest: Option<&str>,
        scope: &HookScope,
    ) -> Result<Approval, ChatError> {
        self.consume(approval_id, digest, Burn::Hook(scope))
    }

    /// The consumption itself. The two callers have different authority over
    /// the approval, so the check is part of the same transaction.
    pub(super) fn consume(
        &self,
        approval_id: &str,
        digest: Option<&str>,
        who: Burn<'_>,
    ) -> Result<Approval, ChatError> {
        self.expire_now()?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = self.now_text();
        let approval = approval_by_id(&transaction, approval_id)?;
        if let Burn::Hook(scope) = who {
            if !scope.covers(&approval) {
                return Err(ChatError::HookScopeMismatch);
            }
        }
        if let Some(digest) = digest {
            if digest != approval.action_digest {
                return Err(ChatError::ApprovalMismatch);
            }
        }
        match approval.status {
            ApprovalStatus::Approved => {}
            ApprovalStatus::Expired => return Err(ChatError::ApprovalExpired),
            ApprovalStatus::Denied | ApprovalStatus::Pending => {
                return Err(ChatError::ApprovalNotApproved)
            }
            ApprovalStatus::Consumed => return Err(ChatError::ApprovalAlreadyDecided),
        }
        transaction.execute(
            "UPDATE approval SET status = 'consumed', consumed_at = ?1, updated_at = ?1
              WHERE id = ?2 AND status = 'approved'",
            params![now, approval_id],
        )?;
        let decider = Decider {
            kind: "agent",
            id: HOOK_CONSUMER,
            level: None,
            reason: None,
            scorer_version: None,
        };
        write_audit(
            &transaction,
            approval_id,
            &approval.action_digest,
            &decider,
            "consumed",
            &now,
        )?;
        let frame = frame_of(&transaction, approval_id, "consumed")?;
        transaction.commit()?;
        self.publish(frame);
        drop(connection);
        self.approval(approval_id)
    }
}
