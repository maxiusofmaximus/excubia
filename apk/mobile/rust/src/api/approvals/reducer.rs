//! The approval reduction: one view per `approval_id`. The status only moves
//! forward (pending, then decided, then consumed), and a frame older than the
//! view it would overwrite is ignored, so a late SSE frame never reopens a card.

use std::collections::BTreeMap;

use super::model::{ChatApproval, STATUSES};
use crate::api::chat::model::ChatEvent;

#[flutter_rust_bridge::frb(opaque)]
pub struct ApprovalReducer {
    approvals: BTreeMap<String, ChatApproval>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_approval_reducer() -> ApprovalReducer {
    ApprovalReducer {
        approvals: BTreeMap::new(),
    }
}

impl ApprovalReducer {
    #[flutter_rust_bridge::frb(sync)]
    pub fn reset(&mut self) {
        self.approvals.clear();
    }

    /// Merge one record: an inbox row, a decision answer or a state read.
    /// Returns whether the stored view changed.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_record(&mut self, record: ChatApproval) -> bool {
        self.merge(record)
    }

    /// Merge one `chat.approval` frame. Any other chat event is ignored.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_event(&mut self, event: ChatEvent) -> bool {
        if event.event != "chat.approval" {
            return false;
        }
        let (Some(approval_id), Some(status)) = (event.approval_id, event.status) else {
            return false;
        };
        if !STATUSES.contains(&status.as_str()) {
            return false;
        }
        self.merge(ChatApproval {
            approval_id,
            conversation_id: (!event.conversation_id.is_empty()).then_some(event.conversation_id),
            seat_id: None,
            pane_id: event.pane_id,
            kind: None,
            tool: None,
            summary: event.summary.unwrap_or_default(),
            cwd: None,
            risk: None,
            command: None,
            question: None,
            status,
            level: event.level,
            escalated_from: event.escalated_from,
            decider_kind: None,
            decider_id: None,
            expires_at: event.expires_at,
            decided_at: None,
            audit: Vec::new(),
            revision: event.revision,
        })
    }

    /// Every view of one conversation (or of all of them), by `approval_id`.
    #[flutter_rust_bridge::frb(sync)]
    pub fn snapshot(&self, conversation_id: Option<String>) -> Vec<ChatApproval> {
        self.approvals
            .values()
            .filter(|approval| {
                conversation_id.is_none() || approval.conversation_id == conversation_id
            })
            .cloned()
            .collect()
    }

    /// The pending approvals the user itself must decide: level `user`, or no
    /// level when the inbox row does not say (never hidden by mistake).
    #[flutter_rust_bridge::frb(sync)]
    pub fn user_pending(&self, conversation_id: Option<String>) -> Vec<ChatApproval> {
        self.snapshot(conversation_id)
            .into_iter()
            .filter(|approval| {
                approval.status == "pending"
                    && matches!(approval.level.as_deref(), None | Some("user"))
            })
            .collect()
    }

    /// The user's approvals that belong to no conversation, in any state: the
    /// inbox shows the pending ones and the ones this session already closed.
    #[flutter_rust_bridge::frb(sync)]
    pub fn user_inbox_unassigned(&self) -> Vec<ChatApproval> {
        self.snapshot(None)
            .into_iter()
            .filter(|approval| {
                approval.conversation_id.is_none()
                    && matches!(approval.level.as_deref(), None | Some("user"))
            })
            .collect()
    }

    fn merge(&mut self, incoming: ChatApproval) -> bool {
        let Some(stored) = self.approvals.get_mut(&incoming.approval_id) else {
            self.approvals
                .insert(incoming.approval_id.clone(), incoming);
            return true;
        };
        let (stored_rank, incoming_rank) = (rank(&stored.status), rank(&incoming.status));
        if incoming_rank < stored_rank {
            return false;
        }
        if incoming_rank == stored_rank && incoming.revision < stored.revision {
            return false;
        }
        let before = stored.clone();
        fold(stored, incoming);
        *stored != before
    }
}

/// `pending` < decided < `consumed`. Decided states share a rank: the newest
/// answer wins between them, which is how a 409 re-read corrects a card.
fn rank(status: &str) -> u8 {
    match status {
        "pending" => 0,
        "consumed" => 2,
        _ => 1,
    }
}

fn fold(stored: &mut ChatApproval, incoming: ChatApproval) {
    stored.status = incoming.status;
    stored.revision = stored.revision.max(incoming.revision);
    replace_some(&mut stored.conversation_id, incoming.conversation_id);
    replace_some(&mut stored.seat_id, incoming.seat_id);
    replace_some(&mut stored.pane_id, incoming.pane_id);
    replace_some(&mut stored.kind, incoming.kind);
    replace_some(&mut stored.tool, incoming.tool);
    replace_some(&mut stored.cwd, incoming.cwd);
    replace_some(&mut stored.risk, incoming.risk);
    replace_some(&mut stored.command, incoming.command);
    replace_some(&mut stored.question, incoming.question);
    replace_some(&mut stored.level, incoming.level);
    replace_some(&mut stored.escalated_from, incoming.escalated_from);
    replace_some(&mut stored.decider_kind, incoming.decider_kind);
    replace_some(&mut stored.decider_id, incoming.decider_id);
    replace_some(&mut stored.expires_at, incoming.expires_at);
    replace_some(&mut stored.decided_at, incoming.decided_at);
    if !incoming.summary.is_empty() {
        stored.summary = incoming.summary;
    }
    if !incoming.audit.is_empty() {
        stored.audit = incoming.audit;
    }
}

/// A frame carries only what changed: a missing field keeps the stored one.
fn replace_some<T>(slot: &mut Option<T>, next: Option<T>) {
    if next.is_some() {
        *slot = next;
    }
}
