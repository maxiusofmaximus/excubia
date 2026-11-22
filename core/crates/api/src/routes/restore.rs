//! The restore-point routes (F2-CORE). Listing a point and what it would change is
//! `READ_STATE`; putting a folder back is `APPROVE`, like the approvals that decide
//! on the host's actions (`auth::authz::restore`). The host does the work through
//! [`AgentActions`]; this module checks the request and shapes the answer.

use std::collections::HashMap;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use excubia_auth::VerifiedDevice;

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::routes::chat::support::ok_object;
use crate::validate::validate_pane_id;
use crate::ApiContext;

/// Why the host did not answer a restore-point request. Never a folder path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreRefusal {
    NotFound,
    Invalid(String),
    Failed(String),
}

impl RestoreRefusal {
    fn into_error(self) -> ApiError {
        match self {
            Self::NotFound => ApiError::coded(
                StatusCode::NOT_FOUND,
                "restore_point_not_found",
                "no restore point with that id",
            ),
            Self::Invalid(message) => ApiError::action("restore_invalid", message),
            Self::Failed(message) => {
                ApiError::coded(StatusCode::CONFLICT, "restore_failed", message)
            }
        }
    }
}

/// `GET /v1/restore/points?agent=<pane_id>` (`READ_STATE`): the points of an agent.
pub async fn points(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let agent = query.get("agent").map(String::as_str).unwrap_or_default();
    if let Err(error) = validate_pane_id(agent) {
        return error.into_response();
    }
    match ctx.actions.restore_points(agent).await {
        Ok(value) => ok_object(value),
        Err(refusal) => refusal.into_error().into_response(),
    }
}

/// `GET /v1/restore/points/{point_id}/files` (`READ_STATE`): the files a restore
/// would add, change or delete, and what the folder leaves out.
pub async fn changes(State(ctx): State<ApiContext>, Path(point_id): Path<String>) -> Response {
    match ctx.actions.restore_changes(&point_id).await {
        Ok(value) => ok_object(value),
        Err(refusal) => refusal.into_error().into_response(),
    }
}

/// `POST /v1/restore/points/{point_id}/restore` (`APPROVE`): puts the folder back
/// as the point saw it. The answer names the point that keeps the state before.
pub async fn restore(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(point_id): Path<String>,
) -> Response {
    let outcome = ctx.actions.restore_point(&point_id).await;
    let result = if outcome.is_ok() { "ok" } else { "failed" };
    ctx.audit.record(
        &AuditEvent::new("restore.undo", Some(&verified.device_id), result).with("point", point_id),
    );
    match outcome {
        Ok(value) => ok_object(value),
        Err(refusal) => refusal.into_error().into_response(),
    }
}
