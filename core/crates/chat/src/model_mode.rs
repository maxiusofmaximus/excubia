//! The approval mode (CORE9-D, RFC §5): who decides an approval. `delegated`
//! lets the policy, the lead and the CTO decide in turn; `always_user` sends
//! every approval to the operator, the trivial reads and tests included.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    #[default]
    Delegated,
    AlwaysUser,
}

impl ApprovalMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delegated => "delegated",
            Self::AlwaysUser => "always_user",
        }
    }

    pub fn from_label(raw: &str) -> Option<Self> {
        match raw {
            "delegated" => Some(Self::Delegated),
            "always_user" => Some(Self::AlwaysUser),
            _ => None,
        }
    }
}

/// One project that overrides the global mode.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectMode {
    pub project_id: String,
    pub mode: ApprovalMode,
}

/// The modes as the operator set them: the global one, and the projects that
/// override it. A project without a row follows the global mode.
#[derive(Clone, Debug, Serialize)]
pub struct ApprovalSettings {
    pub global: ApprovalMode,
    pub projects: Vec<ProjectMode>,
}

/// The answer to a settings request.
#[derive(Clone, Debug, Serialize)]
pub struct ModeChange {
    pub settings: ApprovalSettings,
    /// True when an earlier request with the same `client_id` already applied it.
    pub replayed: bool,
    /// The pending approvals that the change sent up to the operator.
    pub escalated: u64,
}
