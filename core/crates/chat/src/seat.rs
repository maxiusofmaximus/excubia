//! The seat port. CORE6B implements it with real CLIs and the MCP shim; tests
//! use a deterministic fake. The API only knows this trait, never a CLI.

use serde_json::Value;

use crate::model::Turn;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatErrorCode {
    /// No adapter is installed or the selected CLI cannot run.
    Unavailable,
    /// The seat's FIFO queue has no free slot.
    Busy,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct SeatError {
    pub code: SeatErrorCode,
    pub message: String,
}

impl SeatError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: SeatErrorCode::Unavailable,
            message: message.into(),
        }
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self {
            code: SeatErrorCode::Busy,
            message: message.into(),
        }
    }
}

/// One MCP tool invocation, already parsed from JSON-RPC.
#[derive(Clone, Debug)]
pub struct ToolRequest {
    pub tool: String,
    pub arguments: Value,
}

#[derive(Clone, Debug)]
pub struct ToolReply {
    pub result: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolErrorKind {
    Unauthorized,
    Forbidden,
    Invalid,
    Conflict,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub code: &'static str,
    pub field: Option<&'static str>,
    pub message: String,
}

impl ToolError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Unauthorized,
            code: "invalid_tool_token",
            field: None,
            message: message.into(),
        }
    }

    /// A seat that is known but not allowed this action (HTTP 403).
    pub fn forbidden(code: &'static str, field: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Forbidden,
            code,
            field: Some(field),
            message: message.into(),
        }
    }

    pub fn invalid(code: &'static str, field: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Invalid,
            code,
            field: Some(field),
            message: message.into(),
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Conflict,
            code,
            field: None,
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: ToolErrorKind::Unavailable,
            code: "seat_unavailable",
            field: None,
            message: message.into(),
        }
    }
}

/// Who is calling a tool: the durable seat, its project and the conversation
/// the current turn belongs to. `run_id` is the per-turn budget key for Alfred
/// and the active delegation id for a lead.
#[derive(Clone, Debug)]
pub struct SeatActor {
    pub agent_id: String,
    pub role_id: String,
    pub project_id: Option<String>,
    pub conversation_id: String,
    pub run_id: String,
}

/// The port the API enqueues turns into and the MCP shim calls.
///
/// `try_enqueue` is synchronous on purpose: the route must answer `409` when
/// the queue is full or no CLI exists, not accept and fail later. The seat owns
/// every `queued -> running -> completed|failed|interrupted` transition.
///
/// `tool` is async because CORE7 tools start panels and panes; `chat.reply`
/// still completes synchronously inside it.
#[async_trait::async_trait]
pub trait ChatSeat: Send + Sync + 'static {
    fn try_enqueue(&self, turn: Turn) -> Result<(), SeatError>;

    async fn tool(&self, token: &str, request: ToolRequest) -> Result<ToolReply, ToolError>;

    /// Resolve an MCP token to the seat and turn it belongs to. `None` means
    /// the token is unknown or expired.
    fn actor(&self, _token: &str) -> Option<SeatActor> {
        None
    }

    /// Cancel one delegation and stop its turn. Chat state is cancelled even
    /// when the seat has nothing to interrupt.
    fn cancel_delegation(&self, _delegation_id: &str) -> Result<(), SeatError> {
        Ok(())
    }

    /// Kill switch: cancel every active delegation of a conversation.
    fn cancel_conversation(&self, _conversation_id: &str) -> Result<(), SeatError> {
        Ok(())
    }

    /// Stop accepting turns and close the live worker (host shutdown/restart).
    fn shutdown(&self) {}
}
