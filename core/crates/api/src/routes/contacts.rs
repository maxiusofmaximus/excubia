//! `/v1/contacts` (AGENDA-REST): the address book the app keeps, and the phone-book
//! import. The matrix that guards them is `hn-core-auth`'s `authz/agenda.rs`.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agenda_api::{map_error, next_cursor, page_limit};
use crate::contact_api::{self, contact_json, ContactBody, ImportBody};
use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    after: Option<String>,
    limit: Option<usize>,
}

/// The contacts newest first. `q` keeps the contacts whose name or alias contains it;
/// `after` pages past the contact it names.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(query.limit)?;
    let contacts = ctx
        .chat
        .list_contacts(query.q.as_deref(), query.after.as_deref(), limit)
        .map_err(map_error)?;
    Ok(Json(json!({
        "ok": true,
        "contacts": contacts.iter().map(contact_json).collect::<Vec<_>>(),
        "next": next_cursor(contacts.last().map(|contact| contact.id.as_str()), contacts.len(), limit),
    })))
}

pub async fn show(
    State(ctx): State<ApiContext>,
    Path(contact_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let contact = ctx.chat.contact(&contact_id).map_err(map_error)?;
    Ok(Json(json!({"ok": true, "contact": contact_json(&contact)})))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: ContactBody = parse_body(&body)?;
    let contact = contact_api::save(&ctx.chat, &verified.device_id, None, payload)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "contact": contact_json(&contact)})),
    )
        .into_response())
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(contact_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: ContactBody = parse_body(&body)?;
    let contact = contact_api::save(&ctx.chat, &verified.device_id, Some(&contact_id), payload)?;
    Ok(Json(json!({"ok": true, "contact": contact_json(&contact)})))
}

/// The user's own act, so it runs at once; a contact that is gone answers `404`.
pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(contact_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat
        .delete_contact_by_device(&verified.device_id, &contact_id)
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "id": contact_id})))
}

pub async fn import(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: ImportBody = parse_body(&body)?;
    Ok(Json(contact_api::import(
        &ctx.chat,
        &verified.device_id,
        payload,
    )?))
}
