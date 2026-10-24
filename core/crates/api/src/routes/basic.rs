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
    Json(state).into_response()
}
