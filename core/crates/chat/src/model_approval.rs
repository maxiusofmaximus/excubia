//! Approvals (UAC): the request, the decision and the audit trail.
//!
//! `docs/rfc/orchestrator-chat.md` §5 owns the model: an approval is an
//! explicit request bound to one concrete action, it expires, it is used once
//! and only a device with the `APPROVE` grant decides it. The two wire shapes
//! below are the only ones a client sees: the hook polls a state, the device
//! reads the whole record with its audit.

use serde::Serialize;

/// The TTL of an approval when nobody asks for another one.
pub const DEFAULT_TTL_SECONDS: u32 = 120;
/// The longest TTL the host accepts.
pub const MAX_TTL_SECONDS: u32 = 300;
/// How many approvals may wait for a decision in the same conversation.
pub const MAX_PENDING_PER_CONVERSATION: usize = 4;
/// The consumer recorded in the audit when the agent's own hook burns the
/// approval: it is not a device.
pub const HOOK_CONSUMER: &str = "hook";

/// The lifecycle of an approval. `pending` is the only open state; every
/// other one is final and the action never runs again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Consumed,
}

impl ApprovalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            "expired" => Some(Self::Expired),
            "consumed" => Some(Self::Consumed),
            _ => None,
        }
    }

    pub fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
}

/// What the agent wants to do. The kind drives the default risk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalKind {
    Command,
    Network,
    Edit,
    Tool,
}

impl ApprovalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Network => "network",
            Self::Edit => "edit",
            Self::Tool => "tool",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "command" => Some(Self::Command),
            "network" => Some(Self::Network),
            "edit" => Some(Self::Edit),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }

    /// The risk the operator sees. A command or a network call can reach
    /// outside the project, so it is high; an edit or an extra tool is medium.
    pub fn default_risk(self) -> Risk {
        match self {
            Self::Command | Self::Network => Risk::High,
            Self::Edit | Self::Tool => Risk::Medium,
        }
    }
}

/// How dangerous the action is, as the operator sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// The operator's answer. Only a device with the `APPROVE` grant may send
/// one; nothing else in the system can move an approval out of `pending`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Deny,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Deny => "denied",
        }
    }

    /// Accepts the two spellings the clients use (`approve`/`approved`).
    pub fn from_label(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "approve" | "approved" => Some(Self::Approve),
            "deny" | "denied" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// One row of the audit trail: who decided, what action, and when.
#[derive(Clone, Debug, Serialize)]
pub struct ApprovalAudit {
    /// The device that decided (`client_id`), or `host` for a host-driven
    /// transition such as an expiry.
    pub device_id: String,
    pub action_digest: String,
    pub decision: String,
    pub scope: String,
    pub decided_at: String,
}

/// The approval as the operator's device sees it.
#[derive(Clone, Debug, Serialize)]
pub struct Approval {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat_id: Option<String>,
    /// The label the CLI itself used for the agent (OpenCode names its own
    /// agents, so it is not always a seat id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub kind: ApprovalKind,
    pub tool: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub paths: Vec<String>,
    pub risk: Risk,
    pub action_digest: String,
    pub status: ApprovalStatus,
    pub scope: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// Where the approval sits: `policy` once the rules decided it, otherwise
    /// the level that waits for it (`lead`, `user`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// The level it came from, when the policy sent it on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalated_from: Option<String>,
    /// Who decided: `policy` (a rule id), `user` (a device) or `agent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decider_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decider_id: Option<String>,
    /// Why it was sent on, when no level decided it by itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The scorer's probability, when the scorer was asked (RFC §2.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scorer_version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// The redacted payload: what the operator reads to decide. It never
    /// holds the secrets the agent sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_redacted: Option<serde_json::Value>,
    pub audit: Vec<ApprovalAudit>,
}

/// The approval as the hook sees it: the status and when it dies, nothing
/// else. The plugin polls this and must not learn the payload.
#[derive(Clone, Debug, Serialize)]
pub struct ApprovalState {
    pub approval_id: String,
    pub status: ApprovalStatus,
    pub expires_at: String,
}

/// What the caller asks for. The host fills in the conversation, the seat and
/// the delegation it can resolve from the run and the pane.
#[derive(Clone, Debug)]
pub struct NewApproval {
    pub conversation_id: Option<String>,
    pub seat_id: Option<String>,
    pub agent_label: Option<String>,
    pub run_id: Option<String>,
    pub pane_id: Option<String>,
    pub kind: ApprovalKind,
    pub tool: String,
    pub summary: String,
    pub cwd: Option<String>,
    pub paths: Vec<String>,
    pub risk: Option<Risk>,
    pub action_digest: String,
    pub payload_redacted: Option<serde_json::Value>,
    pub ttl_seconds: u32,
}

/// The filters of the approval inbox.
#[derive(Clone, Debug, Default)]
pub struct ApprovalFilter {
    pub status: Option<ApprovalStatus>,
    pub conversation_id: Option<String>,
    pub limit: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_vocabulary_is_closed() {
        for status in [
            ApprovalStatus::Pending,
            ApprovalStatus::Approved,
            ApprovalStatus::Denied,
            ApprovalStatus::Expired,
            ApprovalStatus::Consumed,
        ] {
            assert_eq!(ApprovalStatus::from_db(status.as_str()), Some(status));
        }
        assert_eq!(ApprovalStatus::from_db("nope"), None);
    }

    #[test]
    fn the_decision_accepts_both_spellings() {
        assert_eq!(Decision::from_label("approve"), Some(Decision::Approve));
        assert_eq!(Decision::from_label("APPROVED"), Some(Decision::Approve));
        assert_eq!(Decision::from_label("deny"), Some(Decision::Deny));
        assert_eq!(Decision::from_label("denied"), Some(Decision::Deny));
        assert_eq!(Decision::from_label("maybe"), None);
    }

    #[test]
    fn the_kind_drives_the_default_risk() {
        assert_eq!(ApprovalKind::Command.default_risk(), Risk::High);
        assert_eq!(ApprovalKind::Network.default_risk(), Risk::High);
        assert_eq!(ApprovalKind::Edit.default_risk(), Risk::Medium);
        assert_eq!(ApprovalKind::Tool.default_risk(), Risk::Medium);
    }
}
