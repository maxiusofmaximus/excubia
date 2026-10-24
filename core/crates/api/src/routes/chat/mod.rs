//! `/v1/chat`: the persistent conversation surface of
//! `docs/rfc/orchestrator-chat.md` §3.
//!
//! The repository is synchronous (one SQLite connection) and knows no HTTP;
//! these modules validate, shape and translate. Sending a message enqueues the
//! turn synchronously so the route can answer `409 seat_unavailable` or
//! `409 agent_busy` before accepting it.

mod approvals;
mod bootstrap;
mod conversations;
mod delegations;
mod messages;
mod sections;
mod settings;
mod support;
mod tool;

pub use approvals::{
    consume as consume_approval, create as create_approval, decision as decide_approval,
    list as list_approvals, state as approval_state,
};
pub use bootstrap::bootstrap;
pub use conversations::{create, list as conversations, show, update};
pub use delegations::{cancel as cancel_delegation, cancel_conversation, show as delegation};
pub use messages::{messages, read, send, timeline};
pub use sections::{list as sections, upsert};
pub use settings::{change as change_approval_settings, show as approval_settings};
pub use tool::call as tool;

use axum::response::{IntoResponse, Response};
use hn_core_chat::{ChatError, ChatStore, SeatError, SeatErrorCode};

use crate::error::ApiError;

/// Map a seat refusal to the two `409`s of the contract. The seat already
/// marked the run `failed`; the client can read the timeline or retry with a
/// new `client_id`.
pub(super) fn seat_error(error: SeatError) -> Response {
    eprintln!("chat: seat refused a turn: {error}");
    match error.code {
        SeatErrorCode::Unavailable => ApiError::refused("seat_unavailable").into_response(),
        SeatErrorCode::Busy => ApiError::agent_busy().into_response(),
    }
}

/// The additive `chat` block shared by `GET /v1/state` and the opening SSE
/// frame. One function, so the two shapes cannot diverge.
pub fn state_block(chat: &ChatStore) -> Result<serde_json::Value, ChatError> {
    serde_json::to_value(chat.summary()?).map_err(ChatError::from)
}
