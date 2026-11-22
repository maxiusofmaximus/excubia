use crate::routes::chat::support;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Extension;

use serde_json::json;

use excubia_chat::is_well_formed;

use crate::error::ApiError;
use crate::middleware::HookAuth;
use crate::routes::parse_body;
use crate::ApiContext;

use super::access::scope_of;
use super::bodies::ConsumeBody;

/// `GET /v1/chat/approvals/{id}` (hook): the status of one approval. The
/// plugin polls it; it never learns the payload, and the token must cover the
/// approval it names.
pub async fn state(
    State(ctx): State<ApiContext>,
    Extension(auth): Extension<HookAuth>,
    Path(approval_id): Path<String>,
) -> Response {
    let scope = match scope_of(&auth) {
        Ok(scope) => scope,
        Err(error) => return error.into_response(),
    };
    match ctx.chat.approval_state(&approval_id, scope) {
        Ok(state) => support::ok_object(state),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// `POST /v1/chat/approvals/{id}/consume` (hook): burn the approval just
/// before the action runs. It happens once.
pub async fn consume(
    State(ctx): State<ApiContext>,
    Extension(auth): Extension<HookAuth>,
    Path(approval_id): Path<String>,
    body: Bytes,
) -> Response {
    let scope = match scope_of(&auth) {
        Ok(scope) => scope,
        Err(error) => return error.into_response(),
    };
    let payload: ConsumeBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Some(digest) = payload.action_digest.as_deref() {
        if !is_well_formed(digest) {
            return ApiError::invalid_field(
                "invalid_body",
                "action_digest",
                "action_digest must be sha256: followed by 64 hexadecimal digits",
            )
            .into_response();
        }
    }
    match ctx
        .chat
        .consume_approval(&approval_id, payload.action_digest.as_deref(), scope)
    {
        Ok(approval) => support::ok_object(json!({
            "approval_id": approval.id,
            "status": approval.status.as_str(),
        })),
        Err(error) => support::map_error(error).into_response(),
    }
}
