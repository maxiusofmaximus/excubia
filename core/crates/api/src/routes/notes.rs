//! `/v1/notes` (AGENDA-REST): the notes the app keeps. The matrix that guards them is
//! `hn-core-auth`'s `authz/agenda.rs`; a handler here only shapes the request and reply.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agenda_api::{map_error, next_cursor, page_limit};
use crate::error::ApiError;
use crate::note_api::{self, hit_json, note_json, NoteBody, NoteUpdateBody};
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    before: Option<String>,
    limit: Option<usize>,
}

/// The notes newest first. `q` keeps the notes whose subject or body contains it;
/// `before` pages past the note it names.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(query.limit)?;
    let notes = ctx
        .chat
        .list_notes(query.q.as_deref(), query.before.as_deref(), limit)
        .map_err(map_error)?;
    Ok(Json(json!({
        "ok": true,
        "notes": notes.iter().map(hit_json).collect::<Vec<_>>(),
        "next": next_cursor(notes.last().map(|note| note.id.as_str()), notes.len(), limit),
    })))
}

pub async fn show(
    State(ctx): State<ApiContext>,
    Path(note_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let note = ctx.chat.note(&note_id).map_err(map_error)?;
    Ok(Json(json!({"ok": true, "note": note_json(&note)})))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: NoteBody = parse_body(&body)?;
    let note = note_api::create(&ctx.chat, &verified.device_id, payload)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "note": note_json(&note)})),
    )
        .into_response())
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(note_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: NoteUpdateBody = parse_body(&body)?;
    let note = note_api::change(&ctx.chat, &verified.device_id, &note_id, payload)?;
    Ok(Json(json!({"ok": true, "note": note_json(&note)})))
}

/// The user's own act, so it runs at once; a note that is gone answers `404`.
pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(note_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat
        .delete_note_by_device(&verified.device_id, &note_id)
        .map_err(map_error)?;
    Ok(Json(json!({"ok": true, "id": note_id})))
}
