//! `/v1/schedules` (F4-CORE): the scheduled work a person sets for an oficio. The matrix that
//! guards them is `excubia-core-auth`'s `authz/schedules.rs`; the store runs the rules. The audit
//! names ids and outcomes only: a schedule's text never reaches the log.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{launch_schedule, Claim};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::hire_api;
use crate::routes::chat::support::map_error;
use crate::routes::parse_body;
use crate::schedule_api::ScheduleBody;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    role: Option<String>,
}

/// The schedules, oldest first; `?role=` keeps one oficio's.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let schedules = ctx
        .chat
        .schedules(query.role.as_deref())
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "schedules": schedules})))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: ScheduleBody = parse_body(&body)?;
    let catalog = hire_api::catalog()?;
    hire_api::role(&catalog, &payload.role)?;
    let schedule = ctx
        .chat
        .create_schedule(payload.new_schedule().map_err(map_error)?)
        .map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("schedule.create", Some(&verified.device_id), "created")
            .with("schedule", schedule.id.clone())
            .with("role", schedule.role_id.clone()),
    );
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "schedule": schedule})),
    )
        .into_response())
}

/// An edit: the whole schedule again, so the person reviews what is kept.
pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(schedule_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: ScheduleBody = parse_body(&body)?;
    let schedule = ctx
        .chat
        .update_schedule(&schedule_id, payload.new_schedule().map_err(map_error)?)
        .map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("schedule.update", Some(&verified.device_id), "updated")
            .with("schedule", schedule.id.clone()),
    );
    Ok(Json(json!({"ok": true, "schedule": schedule})))
}

pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat.delete_schedule(&schedule_id).map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("schedule.delete", Some(&verified.device_id), "deleted")
            .with("schedule", schedule_id.clone()),
    );
    Ok(Json(json!({"ok": true, "id": schedule_id})))
}

pub async fn pause(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let schedule = ctx.chat.pause_schedule(&schedule_id).map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("schedule.pause", Some(&verified.device_id), "paused")
            .with("schedule", schedule.id.clone()),
    );
    Ok(Json(json!({"ok": true, "schedule": schedule})))
}

pub async fn resume(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let schedule = ctx.chat.resume_schedule(&schedule_id).map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("schedule.resume", Some(&verified.device_id), "resumed")
            .with("schedule", schedule.id.clone()),
    );
    Ok(Json(json!({"ok": true, "schedule": schedule})))
}

/// The runs of one schedule, newest first: each due instant and what became of it.
pub async fn runs(
    State(ctx): State<ApiContext>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let runs = ctx.chat.schedule_runs(&schedule_id).map_err(map_error)?;
    Ok(Json(json!({"ok": true, "runs": runs})))
}

/// Run the schedule now. The rhythm does not move. A run a brake stops is recorded, not queued.
pub async fn run_now(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(schedule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let answer = match ctx
        .chat
        .claim_schedule_now(&schedule_id)
        .map_err(map_error)?
    {
        Claim::Launch(launch) => {
            let started = launch_schedule(&ctx.chat, ctx.seat.as_ref(), launch)
                .await
                .map_err(ApiError::from)?;
            ctx.audit.record(
                &AuditEvent::new("schedule.run_now", Some(&verified.device_id), "started")
                    .with("schedule", schedule_id.clone())
                    .with("delegation", started.id.clone()),
            );
            json!({
                "ok": true,
                "delegation_id": started.id,
                "status": started.state,
                "depth": started.depth,
            })
        }
        Claim::Recorded(run) => {
            ctx.audit.record(
                &AuditEvent::new(
                    "schedule.run_now",
                    Some(&verified.device_id),
                    run.outcome.as_str(),
                )
                .with("schedule", schedule_id.clone()),
            );
            json!({"ok": true, "run": run})
        }
    };
    Ok(Json(answer))
}
