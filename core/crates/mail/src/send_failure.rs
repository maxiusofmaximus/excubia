//! Why a send failed, in words the API may say (MAIL2A). A closed list of causes
//! and what a retry may do, derived in one place from the error and from whether
//! the POST to Graph was sent. Never the server's text, never a token or an address.

use crate::audit::SendResult;
use crate::error::MailError;

/// The cause of a failed send. Closed: a new cause is a new variant and a new row
/// in [`SendFailure::of`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailCode {
    AccountSuspended,
    AuthExpired,
    Throttled,
    GraphUnavailable,
    BadRequest,
    DraftChanged,
    Interrupted,
    Unknown,
}

/// What a retry may do. `Safe`: the message did not leave. `CheckSent`: it may have
/// left, so look before asking again. `No`: retrying does not help until the cause is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Safe,
    CheckSent,
    No,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFailure {
    pub code: FailCode,
    pub retry: Retry,
}

impl SendFailure {
    /// The draft changed after it was approved: nothing was sent.
    pub const DRAFT_CHANGED: Self = Self {
        code: FailCode::DraftChanged,
        retry: Retry::Safe,
    };
    /// The host stopped while the POST was in flight, so Graph may have taken it.
    pub const INTERRUPTED: Self = Self {
        code: FailCode::Interrupted,
        retry: Retry::CheckSent,
    };
    /// A failure stored before codes existed. Conservative: it may have been sent.
    pub const LEGACY: Self = Self {
        code: FailCode::Unknown,
        retry: Retry::CheckSent,
    };

    /// The failure of a send that `error` ended. `posted` says whether the POST to
    /// Graph went out: then a refusal is final, but a transport error or an
    /// unreadable answer is ambiguous.
    pub fn of(error: &MailError, posted: bool) -> Self {
        let (code, retry) = match error {
            MailError::Graph {
                status: 403, code, ..
            } if code == "ErrorAccountSuspend" => (FailCode::AccountSuspended, Retry::No),
            MailError::AuthRequired => (FailCode::AuthExpired, Retry::Safe),
            MailError::AuthRejected { .. }
            | MailError::AdminConsentRequired { .. }
            | MailError::AppNotRegistered { .. } => (FailCode::AuthExpired, Retry::No),
            MailError::Graph { status: 429, .. } => (FailCode::Throttled, Retry::Safe),
            MailError::Graph {
                status: 500..=599, ..
            } => (FailCode::GraphUnavailable, Retry::Safe),
            MailError::Graph { .. } => (FailCode::BadRequest, Retry::Safe),
            MailError::Transport(_) if posted => (FailCode::GraphUnavailable, Retry::CheckSent),
            MailError::Transport(_) => (FailCode::GraphUnavailable, Retry::Safe),
            _ => (FailCode::Unknown, Retry::CheckSent),
        };
        Self { code, retry }
    }

    /// The audit result this failure is written as.
    pub fn audit_result(self) -> SendResult {
        match self.code {
            FailCode::DraftChanged => SendResult::Stale,
            _ => SendResult::Failed,
        }
    }
}

impl FailCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountSuspended => "account_suspended",
            Self::AuthExpired => "auth_expired",
            Self::Throttled => "throttled",
            Self::GraphUnavailable => "graph_unavailable",
            Self::BadRequest => "bad_request",
            Self::DraftChanged => "draft_changed",
            Self::Interrupted => "interrupted",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "account_suspended" => Some(Self::AccountSuspended),
            "auth_expired" => Some(Self::AuthExpired),
            "throttled" => Some(Self::Throttled),
            "graph_unavailable" => Some(Self::GraphUnavailable),
            "bad_request" => Some(Self::BadRequest),
            "draft_changed" => Some(Self::DraftChanged),
            "interrupted" => Some(Self::Interrupted),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl Retry {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::CheckSent => "check_sent",
            Self::No => "no",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "safe" => Some(Self::Safe),
            "check_sent" => Some(Self::CheckSent),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}
