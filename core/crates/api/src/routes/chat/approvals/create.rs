use crate::routes::chat::support;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::{Extension, Json};

use serde_json::json;

use hn_core_chat::{is_well_formed, ApprovalKind, HookScope, NewApproval, Risk, MAX_TTL_SECONDS};

use crate::error::ApiError;
use crate::middleware::HookAuth;
use crate::routes::parse_body;
use crate::ApiContext;

use super::access::scope_of;
use super::bodies::CreateBody;
use super::bounds::{
    bounded, bounded_list, MAX_LABEL_CHARS, MAX_PATHS, MAX_PATH_CHARS, MAX_SUMMARY_CHARS,
    MAX_TOOL_CHARS,
};

/// `POST /v1/chat/approvals` (hook): open an approval in `pending`.
pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(auth): Extension<HookAuth>,
    body: Bytes,
) -> Response {
    let scope = match scope_of(&auth) {
        Ok(scope) => scope,
        Err(error) => return error.into_response(),
    };
    let payload: CreateBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    // The token is the authority: a body that names another run is refused
    // instead of being silently rewritten.
    if !scope.accepts(
        payload.run_id.as_deref(),
        payload.conversation_id.as_deref(),
    ) {
        return ApiError::forbidden_code("hook_scope_mismatch").into_response();
    }
    let kind = match ApprovalKind::from_db(payload.kind.trim()) {
        Some(kind) => kind,
        None => {
            return ApiError::invalid_field(
                "invalid_body",
                "kind",
                "kind must be command|network|edit|tool",
            )
            .into_response()
        }
    };
    let tool = match bounded(payload.tool.as_deref(), "tool", MAX_TOOL_CHARS, false) {
        Ok(tool) => tool,
        Err(error) => return error.into_response(),
    };
    let summary = match bounded(Some(&payload.summary), "summary", MAX_SUMMARY_CHARS, true) {
        Ok(summary) => summary,
        Err(error) => return error.into_response(),
    };
    if !is_well_formed(&payload.action_digest) {
        return ApiError::invalid_field(
            "invalid_body",
            "action_digest",
            "action_digest must be sha256: followed by 64 hexadecimal digits",
        )
        .into_response();
    }
    let ttl_seconds = match payload.ttl_seconds {
        None => hn_core_chat::DEFAULT_TTL_SECONDS,
        Some(seconds) if (1..=MAX_TTL_SECONDS as u64).contains(&seconds) => seconds as u32,
        Some(_) => {
            return ApiError::invalid_field(
                "invalid_body",
                "ttl_seconds",
                format!("ttl_seconds must be between 1 and {MAX_TTL_SECONDS}"),
            )
            .into_response()
        }
    };
    let risk = match payload.risk.as_deref() {
        None => None,
        Some(raw) => match Risk::from_db(raw.trim()) {
            Some(risk) => Some(risk),
            None => {
                return ApiError::invalid_field(
                    "invalid_body",
                    "risk",
                    "risk must be low|medium|high",
                )
                .into_response()
            }
        },
    };
    let paths = match bounded_list(payload.paths, "paths", MAX_PATHS, MAX_PATH_CHARS) {
        Ok(paths) => paths,
        Err(error) => return error.into_response(),
    };
    let cwd = match bounded(payload.cwd.as_deref(), "cwd", MAX_PATH_CHARS, false) {
        Ok(cwd) => cwd,
        Err(error) => return error.into_response(),
    };
    let pane_id = match bounded(
        payload.pane_id.as_deref(),
        "pane_id",
        MAX_LABEL_CHARS,
        false,
    ) {
        Ok(pane_id) => pane_id,
        Err(error) => return error.into_response(),
    };
    let agent_label = match bounded(
        payload.agent_id.as_deref(),
        "agent_id",
        MAX_LABEL_CHARS,
        false,
    ) {
        Ok(label) => label,
        Err(error) => return error.into_response(),
    };

    let (run_id, conversation_id) = match scope {
        HookScope::Run {
            run_id,
            conversation_id,
            ..
        } => (Some(run_id.clone()), Some(conversation_id.clone())),
        HookScope::Seat { .. } => (None, None),
    };

    let new = NewApproval {
        // The token names the run (or the panel), so the approval cannot be
        // filed under anything else.
        conversation_id,
        seat_id: Some(scope.seat_id().to_string()),
        agent_label,
        run_id,
        pane_id,
        kind,
        tool: tool.unwrap_or_else(|| "unknown".to_string()),
        summary: summary.unwrap_or_default(),
        cwd,
        paths,
        risk,
        action_digest: payload.action_digest,
        payload_redacted: payload.payload_redacted,
        ttl_seconds,
    };
    match ctx.chat.create_approval(new) {
        Ok(approval) => (
            StatusCode::CREATED,
            Json(json!({
                "ok": true,
                "approval_id": approval.id,
                "status": approval.status.as_str(),
                "expires_at": approval.expires_at,
            })),
        )
            .into_response(),
        Err(error) => support::map_error(error).into_response(),
    }
}
