//! `/v1/alerts` (F5): the rules that decide which changes alert the phone, and the mode they run
//! in. The matrix that guards them is `excubia-core-auth`'s `authz/agenda.rs`; a handler here only
//! shapes the request and the reply.

use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{AlertRule, AlertWriter};
use serde_json::{json, Value};

use crate::alert_api::{EnabledBody, ModeBody, RuleBody};
use crate::error::ApiError;
use crate::routes::chat::support::map_error;
use crate::routes::parse_body;
use crate::ApiContext;

pub async fn list(State(ctx): State<ApiContext>) -> Result<Json<Value>, ApiError> {
    let rules = ctx.chat.alert_rules().map_err(map_error)?;
    Ok(Json(json!({
        "ok": true,
        "rules": rules.iter().map(AlertRule::to_json).collect::<Vec<_>>(),
    })))
}

pub async fn show_mode(State(ctx): State<ApiContext>) -> Result<Json<Value>, ApiError> {
    let mode = ctx.chat.alert_mode().map_err(map_error)?;
    Ok(Json(json!({"ok": true, "mode": mode.as_str()})))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: RuleBody = parse_body(&body)?;
    let rule = ctx
        .chat
        .create_alert_rule(
            AlertWriter::Device(&verified.device_id),
            payload.into_new()?,
        )
        .map_err(map_error)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "rule": rule.to_json()})),
    )
        .into_response())
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(rule_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: EnabledBody = parse_body(&body)?;
    let rule = ctx
        .chat
        .set_alert_rule_enabled(
            AlertWriter::Device(&verified.device_id),
            &rule_id,
            payload.enabled,
        )
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "rule": rule.to_json()})))
}

pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(rule_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat
        .delete_alert_rule(AlertWriter::Device(&verified.device_id), &rule_id)
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "id": rule_id})))
}

pub async fn set_mode(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: ModeBody = parse_body(&body)?;
    let mode = ctx
        .chat
        .set_alert_mode(
            AlertWriter::Device(&verified.device_id),
            payload.into_mode()?,
        )
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "mode": mode.as_str()})))
}
