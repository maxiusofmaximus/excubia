//! Stop the open turn of a conversation (`POST …/stop`).
//!
//! Stopping deletes nothing: the user message stays and its turn becomes
//! `interrupted`. The store goes first, so a seat that answers late cannot
//! close the run; the seat is then asked to kill the CLI process of the turn.
//! Approvals the turn already opened stay pending and expire on their own.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::ApiContext;

use super::support;

pub async fn stop_conversation(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
) -> Response {
    match ctx
        .chat
        .stop_conversation_runs(&conversation_id, "stopped_by_user")
    {
        Ok(runs) => {
            if !runs.is_empty() {
                ctx.seat.stop_turns(&conversation_id);
            }
            support::ok_object(json!({"stopped": !runs.is_empty(), "runs": runs.len()}))
        }
        Err(error) => support::map_error(error).into_response(),
    }
}
