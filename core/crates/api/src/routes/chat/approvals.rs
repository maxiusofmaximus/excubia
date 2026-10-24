//! The approval routes (`orchestrator-chat.md` §3 and §5).
//!
//! Two surfaces share one domain and never mix their credentials: the hook
//! creates, polls and consumes an approval with the host token (loopback
//! only), and only a signed device with the `APPROVE` grant decides it. The
//! plugin polls the state; it never sees the payload and it never decides.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use hn_core_auth::VerifiedDevice;
use hn_core_chat::{
    is_well_formed, ApprovalFilter, ApprovalKind, ApprovalStatus, Decision, HookScope, NewApproval,
    Risk, MAX_TTL_SECONDS,
};

use crate::error::ApiError;
use crate::middleware::HookAuth;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support;

const MAX_SUMMARY_CHARS: usize = 400;
const MAX_TOOL_CHARS: usize = 100;
const MAX_PATH_CHARS: usize = 400;
const MAX_PATHS: usize = 32;
const MAX_LABEL_CHARS: usize = 200;

/// The body the hook or the plugin sends to open an approval.
#[derive(Deserialize)]
struct CreateBody {
    kind: String,
    summary: String,
    action_digest: String,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    pane_id: Option<String>,
    /// The CLI's own label for the agent (OpenCode names its own agents).
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    payload_redacted: Option<Value>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
    #[serde(default)]
    risk: Option<String>,
}

/// The body of the operator's answer.
#[derive(Deserialize)]
struct DecisionBody {
    decision: String,
    client_id: String,
    #[serde(default)]
    scope: Option<String>,
}

/// The body of the consumption. The digest is optional: when it is present it
/// must be the one the approval was opened with.
#[derive(Deserialize)]
struct ConsumeBody {
    #[serde(default)]
    action_digest: Option<String>,
}

/// The scope of the token, or the typed refusal of the host token: an
/// approval is always opened, read and burned by the hook of a run or panel.
fn scope_of(auth: &HookAuth) -> Result<&HookScope, ApiError> {
    match auth {
        HookAuth::Scoped(scope) => Ok(scope),
        HookAuth::Host => Err(ApiError::forbidden_code("hook_token_not_scoped")),
    }
}

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

/// `GET /v1/chat/approvals` (signed): the UAC inbox. Without a `status` it
/// answers the pending ones; the resolved ones are asked for by name.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let status = match query.get("status") {
        None => Some(ApprovalStatus::Pending),
        Some(raw) => match ApprovalStatus::from_db(raw.trim()) {
            Some(status) => Some(status),
            None => {
                return ApiError::invalid_field(
                    "invalid_body",
                    "status",
                    "status must be pending|approved|denied|expired|consumed",
                )
                .into_response()
            }
        },
    };
    let limit = match support::limit(&query, 50, 200) {
        Ok(limit) => limit,
        Err(error) => return error.into_response(),
    };
    let filter = ApprovalFilter {
        status,
        conversation_id: query.get("conversation_id").cloned(),
        limit,
    };
    match ctx.chat.approvals(&filter) {
        Ok(approvals) => support::ok_object(json!({ "approvals": approvals })),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// `POST /v1/chat/approvals/{id}/decision` (signed, `APPROVE`): the
/// operator's answer. The same `client_id` always gets the same answer.
pub async fn decision(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(approval_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: DecisionBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let decision = match Decision::from_label(&payload.decision) {
        Some(decision) => decision,
        None => {
            return ApiError::invalid_field(
                "invalid_body",
                "decision",
                "decision must be approve or deny",
            )
            .into_response()
        }
    };
    if let Some(scope) = payload.scope.as_deref() {
        if scope.trim() != "once" {
            return ApiError::invalid_field(
                "invalid_body",
                "scope",
                "the only scope of this version is \"once\"",
            )
            .into_response();
        }
    }
    if let Err(error) = support::client_id(&payload.client_id) {
        return error.into_response();
    }
    match ctx.chat.decide_approval(
        &approval_id,
        decision,
        &payload.client_id,
        &verified.device_id,
    ) {
        Ok(approval) => support::ok_object(json!({ "approval": approval })),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// One bounded text field: no controls, a length ceiling and, when it is
/// required, not blank.
fn bounded(
    raw: Option<&str>,
    field: &'static str,
    max_chars: usize,
    required: bool,
) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw else {
        if required {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} is required"),
            ));
        }
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        if required {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot be blank"),
            ));
        }
        return Ok(None);
    }
    if value.chars().count() > max_chars {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot exceed {max_chars} characters"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot contain control characters"),
        ));
    }
    Ok(Some(value.to_string()))
}

/// A bounded list of paths.
fn bounded_list(
    raw: Vec<String>,
    field: &'static str,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<String>, ApiError> {
    if raw.len() > max_items {
        return Err(ApiError::invalid_field(
            "invalid_body",
            field,
            format!("{field} cannot hold more than {max_items} entries"),
        ));
    }
    let mut paths = Vec::with_capacity(raw.len());
    for value in raw {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > max_chars {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot exceed {max_chars} characters"),
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(ApiError::invalid_field(
                "invalid_body",
                field,
                format!("{field} cannot contain control characters"),
            ));
        }
        paths.push(value.to_string());
    }
    Ok(paths)
}
