//! `/v1/hires` (ONBOARD-T3): the professionals the user has contracted.
//! Contracting only registers them: no pane and no project. Every write is the
//! user's `APPROVE`, which no agent holds.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::{Permission, VerifiedDevice};
use hn_core_chat::{HireFilter, HireOutcome};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::hire_api::{self, hire_json};
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    include_removed: bool,
}

#[derive(Deserialize)]
struct NewHireBody {
    role: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
}

/// `avatar` in an update has three states: absent keeps it, `null` removes it,
/// and a string sets that character.
#[derive(Deserialize)]
struct UpdateBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, deserialize_with = "present_or_null")]
    avatar: Option<Option<String>>,
}

/// Reads a present field as `Some`, even when it is `null`, so that an absent
/// field (`None`, through `default`) stays apart from an explicit `null`.
fn present_or_null<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

/// `can_hire` shows the team's buttons to a device that holds `APPROVE`.
pub async fn list(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let filter = if query.include_removed {
        HireFilter::All
    } else {
        HireFilter::Active
    };
    let hires = ctx.chat.list_hires(filter)?;
    Ok(Json(json!({
        "ok": true,
        "hires": hires.iter().map(hire_json).collect::<Vec<_>>(),
        "can_hire": verified.has(Permission::Approve),
    })))
}

/// `201` for a new hire; the active hire the role already has answers `200`.
pub async fn create(State(ctx): State<ApiContext>, body: Bytes) -> Result<Response, ApiError> {
    let payload: NewHireBody = parse_body(&body)?;
    let catalog = hire_api::catalog()?;
    let role = hire_api::role(&catalog, &payload.role)?;
    let avatar = hire_api::avatar_for(role, payload.avatar.as_deref())?;
    let new = hire_api::new_hire(role, payload.name, avatar);
    let (status, hire) = match ctx.chat.hire(new)? {
        HireOutcome::Created(hire) => (StatusCode::CREATED, hire),
        HireOutcome::Existing(hire) => (StatusCode::OK, hire),
    };
    Ok((status, Json(json!({"ok": true, "hire": hire_json(&hire)}))).into_response())
}

/// Name and avatar are checked before anything is written, so a bad avatar
/// never leaves a new name behind. A body with neither field is refused: there
/// is nothing to change.
pub async fn update(
    State(ctx): State<ApiContext>,
    Path(hire_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: UpdateBody = parse_body(&body)?;
    if payload.name.is_none() && payload.avatar.is_none() {
        return Err(ApiError::invalid_field(
            "invalid_field",
            "name",
            "send a name or an avatar",
        ));
    }
    let avatar = payload
        .avatar
        .map(|avatar| avatar.as_deref().map(hire_api::character).transpose())
        .transpose()?;
    let mut hire = ctx.chat.get_hire(&hire_id)?;
    if let Some(name) = payload.name.as_deref() {
        hire = ctx.chat.rename_hire(&hire_id, name)?;
    }
    if let Some(avatar) = avatar {
        hire = ctx.chat.set_hire_avatar(&hire_id, avatar.as_deref())?;
    }
    Ok(Json(json!({"ok": true, "hire": hire_json(&hire)})))
}

/// Dismissing twice changes nothing: the second answer is the same hire.
pub async fn dismiss(
    State(ctx): State<ApiContext>,
    Path(hire_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let hire = ctx.chat.dismiss_hire(&hire_id)?;
    Ok(Json(json!({"ok": true, "hire": hire_json(&hire)})))
}
