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
use super::approval_rules::{
    approval_by_id, approval_from_columns, approval_row, audit_of, frame_of,
};
use super::approval_settle;

/// The longest single wait of the decision loop, however far the next change
/// is: the loop wakes at least this often, so a clock that jumps is noticed.
const MAX_WAIT: Duration = Duration::from_secs(3600);

/// The shortest single wait, so a deadline already past does not spin the loop.
const MIN_WAIT: Duration = Duration::from_millis(10);

/// What a run waits on: its pending approvals, and how long until one of them
/// changes by itself (a level runs out, the TTL passes), measured from the read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalWaits {
    pub pending: usize,
    pub next_change: Option<Duration>,
}

pub(super) const APPROVAL_SELECT: &str =
    "SELECT id, conversation_id, seat_id, agent_label, run_id, pane_id,
            kind, tool, summary, cwd, paths_json, risk,
            action_digest, status, scope, expires_at,
            decided_by, decided_at, created_at, updated_at,
            payload_redacted, level, escalated_from, decider_kind, decider_id, reason,
            risk_score, scorer_version, headline, shown_at
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
        let reason = (approval.status == ApprovalStatus::Denied)
            .then_some(approval.reason)
            .flatten();
        Ok(ApprovalState {
            approval_id: approval.id,
            status: approval.status,
            expires_at: approval.expires_at,
            reason,
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

    /// Every question of one `tool`, in every state, oldest first. The host filters them by
    /// the digest of its own purpose and reads the decision that closed each one.
    pub fn approvals_for_tool(&self, tool: &str) -> Result<Vec<Approval>, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "{APPROVAL_SELECT}
              WHERE tool = ?1
              ORDER BY rowid ASC"
        ))?;
        let rows = statement.query_map([tool], approval_row)?;
        let mut approvals = Vec::new();
        for row in rows {
            approvals.push(approval_from_columns(row?)?);
        }
        Ok(approvals)
    }

    /// The approved questions of one `tool` that nothing has consumed yet, in the order
    /// they were created. The host applies them and consumes each one (ESTILO3).
    pub fn approved_for_tool(&self, tool: &str) -> Result<Vec<Approval>, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "{APPROVAL_SELECT}
              WHERE tool = ?1 AND status = 'approved'
              ORDER BY rowid ASC"
        ))?;
        let rows = statement.query_map([tool], approval_row)?;
        let mut approvals = Vec::new();
        for row in rows {
            approvals.push(approval_from_columns(row?)?);
        }
        Ok(approvals)
    }

    /// Open an approval and wait for the operator. The approval is consumed
    /// the moment it is allowed, so the action behind it can only run once.
    pub async fn request_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        let ttl_seconds = new.ttl_seconds;
        let approval = self.create_approval(new)?;
        self.settle_request(&approval, ttl_seconds).await
    }

    /// As [`Self::request_approval`], for an action that reaches outside the
    /// workspace (MAIL1D §0). The host places it as never delegable, whatever
    /// seat asks, so only a device with APPROVE can let it through.
    pub async fn request_outside_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        let ttl_seconds = new.ttl_seconds;
        let approval = self.create_outside_approval(new)?;
        self.settle_request(&approval, ttl_seconds).await
    }

    /// As [`Self::request_approval`], for one mailbox rule (F3B). The rule is
    /// created only after the operator's device approves it; nothing else decides
    /// a `mail_rule`.
    pub async fn request_mail_rule_approval(
        &self,
        new: NewApproval,
    ) -> Result<Approval, ChatError> {
        let ttl_seconds = new.ttl_seconds;
        let approval = self.create_mail_rule_approval(new)?;
        self.settle_request(&approval, ttl_seconds).await
    }

    /// The requester waits for the decision with no deadline of its own: the
    /// wait lasts while the card is pending, and the host stops it by dropping
    /// the request (a run that closes, a seat that stops). The TTL starts when a
    /// UI shows the card, not here, so a late app cannot expire it.
    async fn settle_request(
        &self,
        approval: &Approval,
        _ttl_seconds: u32,
    ) -> Result<Approval, ChatError> {
        let decided = self.await_decision(&approval.id).await?;
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
        self.await_until(approval_id, Some(tokio::time::Instant::now() + timeout))
            .await
    }

    /// Wait until the approval is decided, with no deadline of its own (see
    /// `settle_request`).
    pub async fn await_decision(&self, approval_id: &str) -> Result<Approval, ChatError> {
        self.await_until(approval_id, None).await
    }

    async fn await_until(
        &self,
        approval_id: &str,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<Approval, ChatError> {
        let mut events = self.subscribe();
        loop {
            let approval = self.approval(approval_id)?;
            if !approval.status.is_pending() {
                return Ok(approval);
            }
            let now = tokio::time::Instant::now();
            if deadline.is_some_and(|deadline| now >= deadline) {
                return self.approval(approval_id);
            }
            let until_change = self.wait_until_change(approval_id)?;
            let wait = deadline.map_or(until_change, |deadline| (deadline - now).min(until_change));
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

    /// The operator's device showed the card: its TTL starts now, once. A repeat,
    /// or a second device, changes nothing, and a card that is no longer pending
    /// is returned as it is.
    pub fn show_approval(&self, approval_id: &str) -> Result<Approval, ChatError> {
        self.expire_now()?;
        let frame = {
            let mut connection = self.lock();
            let transaction = connection.transaction()?;
            let frame = if approval_settle::show(&transaction, approval_id, self.now_ms())? {
                Some(frame_of(&transaction, approval_id, "shown")?)
            } else {
                None
            };
            transaction.commit()?;
            frame
        };
        if let Some(frame) = frame {
            self.publish(frame);
        }
        self.approval(approval_id)
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

    /// The approvals of these runs that wait for an operator now. The seat's turn
    /// budget pauses on them. Due cards settle first, so an expired one is not
    /// counted; a card of an archived conversation is not waited on either.
    pub fn approval_waits(&self, run_ids: &[String]) -> Result<ApprovalWaits, ChatError> {
        self.waits_where("approval.run_id", run_ids)
    }

    /// The approvals of one delegation that wait for an operator now: the same
    /// rules as [`Self::approval_waits`], keyed by the row the host persisted for
    /// the request (`approval.delegation_id`). Another task's card never counts.
    pub fn delegation_waits(&self, delegation_id: &str) -> Result<ApprovalWaits, ChatError> {
        self.waits_where("approval.delegation_id", &[delegation_id.to_string()])
    }

    /// The waits of the pending approvals whose `column` (a fixed literal, never
    /// an input) is one of `values`.
    fn waits_where(&self, column: &str, values: &[String]) -> Result<ApprovalWaits, ChatError> {
        self.expire_now()?;
        let now = self.now_ms();
        let connection = self.lock();
        let mut waits = ApprovalWaits {
            pending: 0,
            next_change: None,
        };
        let mut statement = connection.prepare(&format!(
            "SELECT approval.id FROM approval
               LEFT JOIN conversation ON conversation.id = approval.conversation_id
              WHERE {column} = ?1 AND approval.status = 'pending'
                AND COALESCE(conversation.archived, 0) = 0"
        ))?;
        for value in values {
            // A held wait (the GitHub code on github.com) pauses the budget like a card.
            waits.pending += self.holds.count(column, value);
            let ids = statement
                .query_map([value], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<String>, _>>()?;
            for id in ids {
                waits.pending += 1;
                if let Some(at) = approval_settle::next_deadline_ms(&connection, &id)? {
                    let left =
                        Duration::from_millis((at - now).max(0) as u64).clamp(MIN_WAIT, MAX_WAIT);
                    waits.next_change = Some(waits.next_change.map_or(left, |s| s.min(left)));
                }
            }
        }
        Ok(waits)
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
