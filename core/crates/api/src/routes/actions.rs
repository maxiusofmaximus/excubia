//! The per-pane actions: respond, keys, read, seen.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::validate::{clamp_read_lines, validate_keys, validate_pane_id, validate_prompt};
use crate::ApiContext;

#[derive(Deserialize)]
struct RespondBody {
    text: String,
}

#[derive(Deserialize)]
struct KeysBody {
    keys: Vec<String>,
    #[serde(default)]
    confirm: bool,
}

#[derive(Deserialize, Default)]
struct ReadBody {
    #[serde(default)]
    lines: Option<i64>,
}

pub async fn respond(
    State(ctx): State<ApiContext>,
    Path(pane_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = validate_pane_id(&pane_id) {
        return error.into_response();
    }
    let payload: RespondBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let text = match validate_prompt(&payload.text) {
        Ok(text) => text,
        Err(error) => return error.into_response(),
    };
    if !ctx.store.agent_exists(&pane_id).await {
        return dead_pane(&pane_id);
    }
    match ctx.actions.respond(&pane_id, &text) {
        Ok(()) => action_result(&ctx, &pane_id).await,
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}

pub async fn keys(
    State(ctx): State<ApiContext>,
    Path(pane_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = validate_pane_id(&pane_id) {
        return error.into_response();
    }
    let payload: KeysBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let keys = match validate_keys(&payload.keys, payload.confirm) {
        Ok(keys) => keys,
        Err(error) => return error.into_response(),
    };
    if !ctx.store.agent_exists(&pane_id).await {
        return dead_pane(&pane_id);
    }
    match ctx.actions.send_keys(&pane_id, &keys, payload.confirm) {
        Ok(()) => action_result(&ctx, &pane_id).await,
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}

pub async fn read(
    State(ctx): State<ApiContext>,
    Path(pane_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = validate_pane_id(&pane_id) {
        return error.into_response();
    }
    let payload: ReadBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let lines = clamp_read_lines(payload.lines);
    if !ctx.store.agent_exists(&pane_id).await {
        return dead_pane(&pane_id);
    }
    match ctx.actions.read(&pane_id, lines) {
        Ok(text) => Json(json!({"text": text, "pane_id": pane_id})).into_response(),
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}

/// Marking an agent seen is an acknowledged no-op on the host (herdr 0.9.x
/// has no way to clear the badge that is not `focus`), so it succeeds for any
/// well-formed pane id and only clears the local badge.
pub async fn seen(State(ctx): State<ApiContext>, Path(pane_id): Path<String>) -> Response {
    if let Err(error) = validate_pane_id(&pane_id) {
        return error.into_response();
    }
    ctx.store.mark_seen(&pane_id).await;
    Json(json!({"ok": true, "pane_id": pane_id})).into_response()
}

/// Stop a live agent: the child is closed with a timeout, the pane leaves the
/// state list (`pane.closed` on SSE) and later actions answer `pane_gone`.
/// Stopping a pane that is already gone is `pane_gone`, not an error of its
/// own; a restored pane has no process to stop (`409`).
pub async fn stop(State(ctx): State<ApiContext>, Path(pane_id): Path<String>) -> Response {
    if let Err(error) = validate_pane_id(&pane_id) {
        return error.into_response();
    }
    let Some(row) = ctx.store.row_of(&pane_id).await else {
        return dead_pane(&pane_id);
    };
    if row.restored {
        return ApiError::refused("the pane has no running agent").into_response();
    }
    match ctx.actions.stop_agent(&pane_id).await {
        Ok(()) => Json(json!({"ok": true, "pane_id": pane_id})).into_response(),
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}

/// A pane that no longer exists is the contract's `pane_gone` typed error;
/// the state list never contains it.
fn dead_pane(pane_id: &str) -> Response {
    ApiError::pane_gone(pane_id).into_response()
}

async fn action_result(ctx: &ApiContext, pane_id: &str) -> Response {
    match ctx.store.row_of(pane_id).await {
        Some(row) => Json(json!({
            "ok": true,
            "result": {
                "pane_id": row.pane_id,
                "agent": row.agent,
                "agent_status": row.status,
            }
        }))
        .into_response(),
        None => dead_pane(pane_id),
    }
}
