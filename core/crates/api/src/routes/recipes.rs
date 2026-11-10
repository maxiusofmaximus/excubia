//! `/v1/recipes` (F5): the recipes a person keeps per oficio. The matrix that
//! guards them is `excubia-core-auth`'s `authz/recipes.rs`; the store runs the guard.
//! The audit names ids only: a recipe's text never reaches the log.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{draft_text, RecipeText};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::hire_api;
use crate::recipe_api::{recipe_json, DraftBody, RecipeBody, RecipeUpdateBody, RunBody};
use crate::routes::chat::support::map_error;
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    role: Option<String>,
}

/// The recipes, newest first; `?role=` keeps one oficio's.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let recipes = ctx
        .chat
        .list_recipes(query.role.as_deref())
        .map_err(map_error)?;
    Ok(Json(json!({
        "ok": true,
        "recipes": recipes.iter().map(recipe_json).collect::<Vec<_>>(),
    })))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: RecipeBody = parse_body(&body)?;
    let catalog = hire_api::catalog()?;
    hire_api::role(&catalog, &payload.role)?;
    let recipe = ctx
        .chat
        .create_recipe(&payload.role, payload.text(), payload.confirm_risks)
        .map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("recipe.create", Some(&verified.device_id), "created")
            .with("recipe", recipe.id.clone())
            .with("role", recipe.role_id.clone()),
    );
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "recipe": recipe_json(&recipe)})),
    )
        .into_response())
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(recipe_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: RecipeUpdateBody = parse_body(&body)?;
    let recipe = ctx
        .chat
        .update_recipe(&recipe_id, payload.text(), payload.confirm_risks)
        .map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("recipe.update", Some(&verified.device_id), "updated")
            .with("recipe", recipe.id.clone()),
    );
    Ok(Json(json!({"ok": true, "recipe": recipe_json(&recipe)})))
}

pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(recipe_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat.delete_recipe(&recipe_id).map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("recipe.delete", Some(&verified.device_id), "deleted")
            .with("recipe", recipe_id.clone()),
    );
    Ok(Json(json!({"ok": true, "id": recipe_id})))
}

/// A draft from a finished delegation. Nothing is kept: the person reads the
/// text, edits it if they want, and saves it with `POST /v1/recipes`.
pub async fn draft(State(ctx): State<ApiContext>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let payload: DraftBody = parse_body(&body)?;
    let source = ctx
        .chat
        .recipe_draft_source(&payload.delegation_id)
        .map_err(map_error)?;
    let catalog = hire_api::catalog()?;
    let role = hire_api::role(&catalog, &source.role_id)?;
    let RecipeText { title, body } = draft_text(&role.title, &source.task);
    Ok(Json(json!({
        "ok": true,
        "draft": {"role_id": source.role_id, "title": title, "body": body},
    })))
}

/// Start the recipe in a conversation: the oficio's hire takes the body from
/// Alfred. The answer names the delegation and where it stands now.
pub async fn run(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(recipe_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: RunBody = parse_body(&body)?;
    let steps = payload.checked_steps()?;
    let run = ctx
        .chat
        .recipe_run_source(&recipe_id, &payload.conversation_id)
        .map_err(map_error)?;
    let started = ctx
        .seat
        .run_recipe(run, steps, payload.plan_id)
        .await
        .map_err(ApiError::from)?;
    let delegation = ctx.chat.delegation(&started.id).map_err(map_error)?;
    ctx.audit.record(
        &AuditEvent::new("recipe.run", Some(&verified.device_id), "started")
            .with("recipe", recipe_id)
            .with("delegation", delegation.id.clone()),
    );
    Ok(Json(json!({
        "ok": true,
        "delegation_id": delegation.id,
        "status": delegation.state,
        "depth": delegation.depth,
    })))
}
