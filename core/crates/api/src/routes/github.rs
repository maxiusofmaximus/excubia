//! `GET /v1/github/link` and `POST /v1/github/link/cancel` (GITHUB-INTEGRACION): the operator's
//! GitHub card reads the link it approved and may cancel it. Signed, `APPROVE`, and the answer
//! is never cached. The code is in this answer only while the login shows it: never in events,
//! polling or the agents' MCP.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::ApiContext;

/// `GET /v1/github/link` (signed, `APPROVE`): the card's state, or `null` without an approved link.
pub async fn link_state(State(ctx): State<ApiContext>) -> Response {
    let link = ctx.seat.github_link_card();
    no_store(Json(json!({ "ok": true, "link": link })).into_response())
}

#[derive(Deserialize)]
struct CancelBody {
    approval_id: String,
}

/// `POST /v1/github/link/cancel` (signed, `APPROVE`): stops the link of one question. The code
/// is forgotten with it; a question the link does not serve cancels nothing.
pub async fn link_cancel(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: CancelBody = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return ApiError::invalid_body().into_response(),
    };
    let cancelled = ctx.seat.github_link_cancel(&payload.approval_id);
    no_store(Json(json!({ "ok": true, "cancelled": cancelled })).into_response())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    response
}
