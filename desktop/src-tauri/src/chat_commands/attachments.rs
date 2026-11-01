//! Attachment commands. The webview names a staged file by its id and sends
//! the bytes as base64; it never gets a disk path, and reads return bytes only.

use herdr_desktop_core::chat::{Attachment, AttachmentContent, ChatFailure};
use tauri::State;

use super::{client, shared, Shared};

#[tauri::command]
pub async fn chat_attachment_create(
    state: State<'_, Shared>,
    conversation_id: String,
    client_id: String,
    name: String,
    size: u64,
    sha256: String,
) -> Result<Attachment, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_attachment_create(&conversation_id, &client_id, &name, size, &sha256)
        .await
}

#[tauri::command]
pub async fn chat_attachment_chunk(
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
    offset: u64,
    data: String,
) -> Result<u64, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_attachment_chunk(&conversation_id, &attachment_id, offset, &data)
        .await
}

#[tauri::command]
pub async fn chat_attachment_complete(
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
) -> Result<Attachment, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_attachment_complete(&conversation_id, &attachment_id)
        .await
}

#[tauri::command]
pub async fn chat_attachment_delete(
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_attachment_delete(&conversation_id, &attachment_id)
        .await
}

#[tauri::command]
pub async fn chat_attachment_content(
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
) -> Result<AttachmentContent, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_attachment_content(&conversation_id, &attachment_id)
        .await
}
