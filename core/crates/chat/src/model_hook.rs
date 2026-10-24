//! Hook tokens: what an agent's own hook may touch, and for how long.
//!
//! A token is minted when the host launches the work it belongs to — one turn
//! for a chat run, one process for a resident panel — and it is revoked when
//! that work closes. It is not a device credential: it only reaches the three
//! approval routes, over loopback, and it names exactly one scope.

use crate::model_approval::Approval;

/// What one hook token covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookScope {
    /// One chat turn: the approval must belong to this run and conversation.
    Run {
        seat_id: String,
        run_id: String,
        conversation_id: String,
    },
    /// A resident panel, which serves many turns: the approval must belong to
    /// this seat.
    Seat { seat_id: String },
}

impl HookScope {
    /// The token of one chat run.
    pub fn run(run_id: &str, conversation_id: &str, seat_id: &str) -> Self {
        Self::Run {
            seat_id: seat_id.to_string(),
            run_id: run_id.to_string(),
            conversation_id: conversation_id.to_string(),
        }
    }

    /// The token of a resident panel: the whole seat, not one turn.
    pub fn seat(seat_id: &str) -> Self {
        Self::Seat {
            seat_id: seat_id.to_string(),
        }
    }

    /// The seat that owns the token, in both shapes.
    pub fn seat_id(&self) -> &str {
        match self {
            Self::Run { seat_id, .. } | Self::Seat { seat_id } => seat_id,
        }
    }

    /// Whether this token may open, read or burn this approval.
    pub fn covers(&self, approval: &Approval) -> bool {
        if approval.seat_id.as_deref() != Some(self.seat_id()) {
            return false;
        }
        match self {
            Self::Run {
                run_id,
                conversation_id,
                ..
            } => {
                approval.run_id.as_deref() == Some(run_id.as_str())
                    && approval.conversation_id.as_deref() == Some(conversation_id.as_str())
            }
            Self::Seat { .. } => true,
        }
    }

    /// Whether a body that claims its own run or conversation agrees with the
    /// token. The token is the authority: a declared value that contradicts it
    /// is refused, an omitted one is filled from it.
    pub fn accepts(&self, run_id: Option<&str>, conversation_id: Option<&str>) -> bool {
        fn agrees(declared: Option<&str>, own: &str) -> bool {
            match declared {
                None => true,
                Some(value) => value.is_empty() || value == own,
            }
        }
        match self {
            Self::Run {
                run_id: own_run,
                conversation_id: own_conversation,
                ..
            } => agrees(run_id, own_run) && agrees(conversation_id, own_conversation),
            // A panel owns no turn: a body that names one is not its own.
            Self::Seat { .. } => {
                run_id.is_none_or(str::is_empty) && conversation_id.is_none_or(str::is_empty)
            }
        }
    }
}

/// What the host found for a presented token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookTokenUse {
    /// A live token and the scope it covers.
    Live(HookScope),
    /// A token of a run or panel that already closed.
    Revoked,
    /// No such token: not a hook credential at all.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_approval::{Approval, ApprovalKind, ApprovalStatus, Risk};

    fn approval(seat: &str, run: &str, conversation: &str) -> Approval {
        Approval {
            id: "apr_1".to_string(),
            conversation_id: Some(conversation.to_string()),
            seat_id: Some(seat.to_string()),
            agent_label: None,
            run_id: Some(run.to_string()),
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "cargo test".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: Risk::High,
            action_digest: "sha256:0".to_string(),
            status: ApprovalStatus::Pending,
            scope: "once".to_string(),
            expires_at: "2026-10-22T10:00:00.000Z".to_string(),
            decided_by: None,
            decided_at: None,
            level: None,
            escalated_from: None,
            decider_kind: None,
            decider_id: None,
            reason: None,
            risk_score: None,
            scorer_version: None,
            created_at: "2026-10-22T09:58:00.000Z".to_string(),
            updated_at: "2026-10-22T09:58:00.000Z".to_string(),
            payload_redacted: None,
            audit: Vec::new(),
        }
    }

    #[test]
    fn a_run_token_covers_one_turn() {
        let scope = HookScope::run("run_a", "cnv_1", "ag_alfred");
        assert!(scope.covers(&approval("ag_alfred", "run_a", "cnv_1")));
        assert!(!scope.covers(&approval("ag_alfred", "run_b", "cnv_1")));
        assert!(!scope.covers(&approval("ag_other", "run_a", "cnv_1")));
    }

    #[test]
    fn a_panel_token_covers_the_whole_seat() {
        let scope = HookScope::seat("ag_lead");
        assert!(scope.covers(&approval("ag_lead", "run_x", "cnv_1")));
        assert!(!scope.covers(&approval("ag_other", "run_x", "cnv_1")));
    }

    #[test]
    fn a_body_cannot_claim_another_run() {
        let scope = HookScope::run("run_a", "cnv_1", "ag_alfred");
        assert!(scope.accepts(None, None));
        assert!(scope.accepts(Some(""), Some("")));
        assert!(scope.accepts(Some("run_a"), Some("cnv_1")));
        assert!(!scope.accepts(Some("run_b"), None));
        assert!(!scope.accepts(None, Some("cnv_2")));
    }

    #[test]
    fn a_panel_accepts_no_run_from_the_body() {
        let scope = HookScope::seat("ag_lead");
        assert!(scope.accepts(None, None));
        assert!(scope.accepts(Some(""), Some("")));
        assert!(!scope.accepts(Some("run_a"), None));
    }
}
