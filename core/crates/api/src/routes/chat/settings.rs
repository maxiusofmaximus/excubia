//! The approval mode (CORE9-D, RFC §5 and §7): read the modes, and change
//! them with a signed device that holds `APPROVE`. No agent surface reaches
//! these routes: the MCP tools and the hook routes have no way to write them.

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use hn_core_auth::VerifiedDevice;
use hn_core_chat::ApprovalMode;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support;

#[derive(Deserialize)]
struct ModeBody {
    client_id: String,
    mode: String,
    /// Absent: the change is for the global mode.
    project_id: Option<String>,
}

/// `GET /v1/chat/approvals/settings` (signed, `READ_STATE`): the global mode
/// and the projects that override it.
pub async fn show(State(ctx): State<ApiContext>) -> Response {
    match ctx.chat.approval_settings() {
        Ok(settings) => support::ok_object(json!({ "settings": settings })),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// `POST /v1/chat/approvals/settings` (signed, `APPROVE`): set the global mode,
/// or the mode of one project. The signing device is the decider of the audit;
/// the same `client_id` changes nothing twice.
pub async fn change(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Response {
    let payload: ModeBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let Some(mode) = ApprovalMode::from_label(payload.mode.trim()) else {
        return ApiError::invalid_field(
            "invalid_mode",
            "mode",
            "mode must be delegated or always_user",
        )
        .into_response();
    };
    if let Err(error) = support::client_id(&payload.client_id) {
        return error.into_response();
    }
    match ctx.chat.set_approval_mode(
        &payload.client_id,
        &verified.device_id,
        payload.project_id.as_deref(),
        mode,
    ) {
        Ok(change) => support::ok_object(json!({
            "settings": change.settings,
            "replayed": change.replayed,
            "escalated": change.escalated,
        })),
        Err(error) => support::map_error(error).into_response(),
    }
}
