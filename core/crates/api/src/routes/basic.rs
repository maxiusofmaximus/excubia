//! Liveness and the full state read model.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::error::ApiError;
use crate::ApiContext;

/// Liveness, with nothing an attacker could use: no version, no pane names.
pub async fn health(State(ctx): State<ApiContext>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "revision": ctx.store.revision().await,
        "devices": ctx.devices.len(),
        "streams": ctx.streams.total(),
    }))
}

/// The one state read model, shared by `GET /v1/state` and the opening SSE
/// frame: a second shape for one concept would be a contract break. The `chat`
/// block is additive (CORE6B); the top-level `revision` is still the state's.
pub async fn state(State(ctx): State<ApiContext>) -> Response {
    let payload = ctx.store.payload().await;
    let mut state = match serde_json::to_value(&payload) {
        Ok(state) => state,
        Err(error) => return ApiError::internal(error).into_response(),
    };
    match super::chat::state_block(&ctx.chat) {
        Ok(chat) => state["chat"] = chat,
        Err(error) => return ApiError::internal(error).into_response(),
    }
    // The calendar counts are additive too: no titles, only what is left of today.
    match ctx.chat.calendar_summary() {
        Ok(summary) => {
            state["calendar_today"] = json!(summary.today);
            state["calendar_next"] = json!(summary
                .next
                .map(|next| json!({ "id": next.id, "start_utc": next.start_utc })));
            state["reminders_due"] = json!(summary.reminders_due);
        }
        Err(error) => return ApiError::internal(error).into_response(),
    }
    // The alert counts are additive: the mode and how many rules are on, never the rules.
    match ctx.chat.alert_summary() {
        Ok(summary) => {
            state["alert_rules"] = json!({
                "mode": summary.mode.as_str(),
                "enabled_count": summary.enabled_count,
            });
        }
        Err(error) => return ApiError::internal(error).into_response(),
    }
    // Additive: `unavailable`, `suspended` (a local model holds the machine) or `ready`.
    state["voice"] = json!(ctx.voice.state());
    Json(state).into_response()
}
