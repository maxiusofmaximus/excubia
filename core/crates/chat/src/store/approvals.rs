//! Approvals: the reads, the wait for the decision and the clock's moves. The
//! creation of a row lives in [`super::approval_create`].
//!
//! One approval backs one concrete action. It is created `pending` (or decided
//! at once by the policy), it climbs its levels on their clock, it dies with
//! its TTL, and it is consumed once, just before the action runs
//! (`orchestrator-chat.md` §5). The decisions live in
//! [`super::approval_decisions`] (the operator and the consumption) and in
//! [`super::approval_delegation`] (a lead or the CTO). The moves by the clock
//! live in [`super::approval_settle`].
//!
//! The rules of a row live in [`super::approval_rules`].

use std::time::Duration;

use rusqlite::params;

use crate::error::ChatError;
use crate::model::ChatFrame;
use crate::model_approval::{Approval, ApprovalFilter, ApprovalState, ApprovalStatus, NewApproval};
use crate::model_hook::HookScope;

use super::approval_decisions::Burn;
use super::approval_rules::{approval_by_id, approval_from_columns, approval_row, audit_of};
use super::approval_settle;

/// The extra time the host waits after the TTL before it gives up on the
/// decision: the approval expires by itself, the wait only confirms it.
const DECISION_SLACK: Duration = Duration::from_secs(5);

/// The longest single wait of the decision loop, however far the next change
/// is: the loop wakes at least this often, so a clock that jumps is noticed.
const MAX_WAIT: Duration = Duration::from_secs(3600);

/// The shortest single wait, so a deadline already past does not spin the loop.
const MIN_WAIT: Duration = Duration::from_millis(10);

pub(super) const APPROVAL_SELECT: &str =
    "SELECT id, conversation_id, seat_id, agent_label, run_id, pane_id,
            kind, tool, summary, cwd, paths_json, risk,
            action_digest, status, scope, expires_at,
            decided_by, decided_at, created_at, updated_at,
            payload_redacted, level, escalated_from, decider_kind, decider_id, reason,
            risk_score, scorer_version
       FROM approval";

impl super::ChatStore {
    /// The approval as the operator's device sees it, with its audit trail.
    pub fn approval(&self, approval_id: &str) -> Result<Approval, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        let mut approval = approval_by_id(&connection, approval_id)?;
        approval.audit = audit_of(&connection, approval_id)?;
        Ok(approval)
    }

    /// The approval as the hook sees it: the status and the deadline, nothing
    /// else. The plugin polls this and must not learn the payload, and the
    /// token must cover the approval it names.
    pub fn approval_state(
        &self,
        approval_id: &str,
        scope: &HookScope,
    ) -> Result<ApprovalState, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        let approval = approval_by_id(&connection, approval_id)?;
        if !scope.covers(&approval) {
            return Err(ChatError::HookScopeMismatch);
        }
        Ok(ApprovalState {
            approval_id: approval.id,
            status: approval.status,
            expires_at: approval.expires_at,
        })
    }

    /// The approval inbox: the pending ones by default, the resolved ones on
    /// request.
    pub fn approvals(&self, filter: &ApprovalFilter) -> Result<Vec<Approval>, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        let status = filter.status.map(|status| status.as_str().to_string());
        let conversation_id = filter.conversation_id.clone();
        let limit = filter.limit.clamp(1, 200) as i64;
        let mut statement = connection.prepare(&format!(
            "{APPROVAL_SELECT}
              WHERE (?1 IS NULL OR status = ?1)
                AND (?2 IS NULL OR conversation_id = ?2)
              ORDER BY created_at DESC, id DESC
              LIMIT ?3"
        ))?;
        let rows = statement.query_map(params![status, conversation_id, limit], approval_row)?;
        let mut approvals = Vec::new();
        for row in rows {
            let mut approval = approval_from_columns(row?)?;
            approval.audit = audit_of(&connection, &approval.id)?;
            approvals.push(approval);
        }
        Ok(approvals)
    }

    /// Open an approval and wait for the operator. The approval is consumed
    /// the moment it is allowed, so the action behind it can only run once.
    pub async fn request_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        let wait = Duration::from_secs(new.ttl_seconds as u64) + DECISION_SLACK;
        let approval = self.create_approval(new)?;
        let decided = self.await_approval(&approval.id, wait).await?;
        if decided.status == ApprovalStatus::Approved {
            return self.consume(&approval.id, Some(&approval.action_digest), Burn::Host);
        }
        Ok(decided)
    }

    /// Wait until the approval is decided. The wait is event driven, and it
    /// wakes at the next moment the approval changes by itself (a level runs
    /// out, the TTL passes), so an escalation is never waited out.
    pub async fn await_approval(
        &self,
        approval_id: &str,
        timeout: Duration,
    ) -> Result<Approval, ChatError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut events = self.subscribe();
        loop {
            let approval = self.approval(approval_id)?;
            if !approval.status.is_pending() {
                return Ok(approval);
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return self.approval(approval_id);
            }
            let until_change = self.wait_until_change(approval_id)?;
            let wait = (deadline - now).min(until_change);
            match tokio::time::timeout(wait, events.recv()).await {
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    return self.approval(approval_id)
                }
                // A frame, a lag or an elapsed wait: the loop reads again and
                // settles whatever the clock moved.
                Ok(Ok(_))
                | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_)))
                | Err(_) => continue,
            }
        }
    }

    /// How long the decision loop may sleep before the approval changes by
    /// itself, measured on the store's clock.
    fn wait_until_change(&self, approval_id: &str) -> Result<Duration, ChatError> {
        let next = {
            let connection = self.lock();
            approval_settle::next_deadline_ms(&connection, approval_id)?
        };
        let Some(at) = next else {
            return Ok(MAX_WAIT);
        };
        let remaining = Duration::from_millis((at - self.now_ms()).max(0) as u64);
        Ok(remaining.clamp(MIN_WAIT, MAX_WAIT))
    }

    /// How many approvals wait for a decision: the `pending_approvals`
    /// counter of `/v1/state` and of the bootstrap.
    pub fn pending_approval_count(&self) -> Result<u64, ChatError> {
        let connection = self.lock();
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM approval WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    /// Settle the approvals whose deadline passed. The transitions are lazy but
    /// total: every read, decision and consumption settles first.
    pub(super) fn expire_now(&self) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let frames: Vec<ChatFrame> = approval_settle::settle_due(&transaction, self.now_ms())?;
        transaction.commit()?;
        for frame in frames {
            self.publish(frame);
        }
        Ok(())
    }
}
