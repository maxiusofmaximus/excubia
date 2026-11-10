use crate::routes::chat::support;
use axum::response::IntoResponse;
use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Extension;

use serde_json::{json, Value};

use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{ApprovalFilter, ApprovalStatus, Decision};

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
        Ok(approval) => {
            if was_pending {
                ctx.mail.approval_decided(&approval);
            }
            support::ok_object(json!({ "approval": approval.view(reader_of(&verified)) }))
        }
        Err(error) => support::map_error(error).into_response(),
    }
}
