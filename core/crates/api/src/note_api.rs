//! The notes of the agenda over REST (AGENDA-REST): the bodies a device sends, the JSON
//! it reads, and the store calls behind them. The tool parser checks every field.

use excubia_core_chat::{ChatStore, Note, NoteHit};
use excubia_core_seat::{NoteCall, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agenda_api::{self, map_error};
use crate::error::ApiError;

/// A note a device creates. `title` is its subject; `message_id` names the message that
/// made it, which must exist.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteBody {
    #[serde(default)]
    title: Option<String>,
    body: String,
    #[serde(default)]
    message_id: Option<String>,
}

/// An edit: an absent field keeps its value, and an empty `title` clears the subject.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteUpdateBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

pub fn create(chat: &ChatStore, device_id: &str, payload: NoteBody) -> Result<Note, ApiError> {
    let arguments = json!({"body": payload.body, "subject": payload.title});
    let ToolCall::Note(NoteCall::Save { subject, body }) =
        agenda_api::parse("note.save", arguments)?
    else {
        return Err(ApiError::internal("note.save parsed to another call"));
    };
    chat.save_note_by_device(device_id, subject, body, payload.message_id)
        .map_err(map_error)
}

pub fn change(
    chat: &ChatStore,
    device_id: &str,
    note_id: &str,
    payload: NoteUpdateBody,
) -> Result<Note, ApiError> {
    let arguments = json!({"id": note_id, "subject": payload.title, "body": payload.body});
    let ToolCall::Note(NoteCall::Update { id, subject, body }) =
        agenda_api::parse("note.update", arguments)?
    else {
        return Err(ApiError::internal("note.update parsed to another call"));
    };
    chat.update_note_by_device(device_id, &id, subject, body)
        .map_err(map_error)
}

pub fn note_json(note: &Note) -> Value {
    json!({
        "id": note.id,
        "subject": note.subject,
        "body": note.body,
        "conversation_id": note.conversation_id,
        "message_id": note.message_id,
        "created_at": note.created_at,
        "updated_at": note.updated_at,
    })
}

pub fn hit_json(hit: &NoteHit) -> Value {
    json!({
        "id": hit.id,
        "subject": hit.subject,
        "excerpt": hit.excerpt,
        "updated_at": hit.updated_at,
    })
}
