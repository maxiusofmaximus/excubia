//! Delegation detail and cancellation (`orchestrator-chat.md` §3, §4).
//!
//! Cancelling never deletes history: the delegation becomes `cancelled`, its
//! run is interrupted and the card keeps the last state. The seat is asked to
//! stop the panel's turn after the durable state is already safe.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiContext;

use super::support;

pub async fn show(State(ctx): State<ApiContext>, Path(delegation_id): Path<String>) -> Response {
    match ctx.chat.delegation_detail(&delegation_id) {
        Ok(detail) => support::ok_object(detail),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn cancel(State(ctx): State<ApiContext>, Path(delegation_id): Path<String>) -> Response {
    match ctx.chat.cancel_delegation(&delegation_id) {
        Ok(_) => {
            if let Err(error) = ctx.seat.cancel_delegation(&delegation_id) {
                eprintln!("chat: could not interrupt delegation {delegation_id}: {error}");
            }
            match ctx.chat.delegation_detail(&delegation_id) {
                Ok(detail) => support::ok_object(detail),
                Err(error) => support::map_error(error).into_response(),
            }
        }
        Err(error) => support::map_error(error).into_response(),
    }
}

/// The conversation kill switch: every active delegation is cancelled; the
/// history and the conversation itself stay.
pub async fn cancel_conversation(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
) -> Response {
    match ctx.chat.cancel_conversation(&conversation_id) {
        Ok(cancelled) => {
            if let Err(error) = ctx.seat.cancel_conversation(&conversation_id) {
                eprintln!("chat: could not interrupt conversation {conversation_id}: {error}");
            }
            let ids: Vec<String> = cancelled
                .into_iter()
                .map(|delegation| delegation.id)
                .collect();
            support::ok_object(json!({
                "conversation_id": conversation_id,
                "cancelled": ids,
            }))
        }
        Err(error) => support::map_error(error).into_response(),
    }
}
