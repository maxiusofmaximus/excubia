//! `/v1/plans` (F3): the plans the agents propose. Reading them is `READ_STATE`;
//! answering one is `APPROVE`, checked by the authz matrix before it gets here.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Response};
use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{PlanDecision, PlanStatus};
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::chat::support::{map_error, ok_object};
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
struct NoteBody {
    #[serde(default)]
    note: Option<String>,
}

/// `GET /v1/plans?status=` (signed, `READ_STATE`): the plans, newest first.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let status = match query
        .get("status")
        .map(|raw| PlanStatus::from_db(raw.trim()))
    {
        None => None,
        Some(Some(status)) => Some(status),
        Some(None) => {
            return ApiError::invalid_field(
                "invalid_body",
                "status",
                "status must be proposed|approved|rejected|changes_requested",
            )
            .into_response()
        }
    };
    match ctx.chat.plans(status) {
        Ok(plans) => ok_object(json!({ "plans": plans })),
        Err(error) => map_error(error).into_response(),
    }
}

/// `POST /v1/plans/{id}/approve` (signed, `APPROVE`).
pub async fn approve(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(plan_id): Path<String>,
    body: Bytes,
) -> Response {
    answer(&ctx, &verified, &plan_id, PlanDecision::Approve, &body)
}

/// `POST /v1/plans/{id}/reject` (signed, `APPROVE`).
pub async fn reject(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(plan_id): Path<String>,
    body: Bytes,
) -> Response {
    answer(&ctx, &verified, &plan_id, PlanDecision::Reject, &body)
}

/// `POST /v1/plans/{id}/request_changes` (signed, `APPROVE`): the note, at most
/// 300 characters, is what the agent reads before it proposes again.
pub async fn request_changes(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(plan_id): Path<String>,
    body: Bytes,
) -> Response {
    answer(
        &ctx,
        &verified,
        &plan_id,
        PlanDecision::RequestChanges,
        &body,
    )
}

fn answer(
    ctx: &ApiContext,
    verified: &VerifiedDevice,
    plan_id: &str,
    decision: PlanDecision,
    body: &Bytes,
) -> Response {
    let note = if body.is_empty() {
        None
    } else {
        match parse_body::<NoteBody>(body) {
            Ok(payload) => payload.note,
            Err(error) => return error.into_response(),
        }
    };
    match ctx
        .chat
        .decide_plan(plan_id, decision, note, &verified.device_id)
    {
        Ok(plan) => ok_object(json!({ "plan": plan })),
        Err(error) => map_error(error).into_response(),
    }
}
