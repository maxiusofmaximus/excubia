use crate::routes::chat::support;
use axum::response::IntoResponse;
use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Extension;

use serde_json::{json, Value};

use excubia_auth::VerifiedDevice;
use excubia_chat::{ApprovalFilter, ApprovalStatus, Decision};

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::access::reader_of;
use super::bodies::DecisionBody;
use super::bounds::{bounded_list, MAX_EXCLUDE, MAX_EXCLUDE_CHARS};

/// `GET /v1/chat/approvals` (signed): the UAC inbox. Without a `status` it
/// answers the pending ones; the resolved ones are asked for by name.
pub async fn list(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
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
    let reader = reader_of(&verified);
    match ctx.chat.approvals(&filter) {
        Ok(approvals) => {
            let views: Vec<Value> = approvals
                .iter()
                .map(|approval| approval.view(reader))
                .collect();
            support::ok_object(json!({ "approvals": views }))
        }
        Err(error) => support::map_error(error).into_response(),
    }
}

/// `POST /v1/chat/approvals/{id}/decision` (signed, `APPROVE`): the
/// operator's answer. The same `client_id` always gets the same answer. `exclude`
/// leaves out messages of a mail batch.
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
    // «task»: the same kind of action of this seat, in this folder, until its task ends.
    let task_scope = match payload.scope.as_deref().map(str::trim) {
        None | Some("once") => false,
        Some("task") if decision == Decision::Approve => {
            match ctx.chat.task_scope_allowed(&approval_id) {
                Ok(true) => true,
                Ok(false) => {
                    return ApiError::invalid_field(
                        "task_scope_unavailable",
                        "scope",
                        "only an action of a task, inside its folder and not a mail action, can be allowed for the whole task",
                    )
                    .into_response()
                }
                Err(error) => return support::map_error(error).into_response(),
            }
        }
        Some(_) => {
            return ApiError::invalid_field(
                "invalid_body",
                "scope",
                "scope is \"once\", or \"task\" with an approval",
            )
            .into_response()
        }
    };
    if decision == Decision::Deny {
        if let Some(reason) = payload.reason.as_deref() {
            if let Err(error) = ctx.chat.note_denial_reason(&approval_id, reason) {
                return support::map_error(error).into_response();
            }
        }
    }
    if let Err(error) = support::client_id(&payload.client_id) {
        return error.into_response();
    }
    let exclude = match bounded_list(payload.exclude, "exclude", MAX_EXCLUDE, MAX_EXCLUDE_CHARS) {
        Ok(exclude) => exclude,
        Err(error) => return error.into_response(),
    };
    // Only the decision that moves a pending approval is new; a repeat of it is told nothing.
    let was_pending = matches!(
        ctx.chat.approval(&approval_id).map(|before| before.status),
        Ok(ApprovalStatus::Pending)
    );
    match ctx.chat.decide_approval_excluding(
        &approval_id,
        decision,
        &exclude,
        &payload.client_id,
        &verified.device_id,
    ) {
        Ok(mut approval) => {
            if was_pending {
                ctx.mail.approval_decided(&approval);
            }
            if task_scope {
                match ctx.chat.grant_task_scope(&approval.id) {
                    // The answer carries the scope the host now holds, so the card says it at once.
                    Ok(true) => approval.scope = "task".to_string(),
                    Ok(false) => {}
                    Err(error) => return support::map_error(error).into_response(),
                }
            }
            support::ok_object(json!({ "approval": approval.view(reader_of(&verified)) }))
        }
        Err(error) => support::map_error(error).into_response(),
    }
}
