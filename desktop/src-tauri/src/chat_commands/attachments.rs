//! Attachment commands. The webview names a staged file by its id and sends
//! the bytes as base64; it never gets a disk path, and reads return bytes only.

use excubia_desktop_core::chat::{Attachment, AttachmentContent, ChatFailure};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{client, documents, shared, Shared};
use crate::file_dialog::chosen;

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

/// Downloads a document and opens it with the system's program. The ids are
/// checked by the client before any name or file is made from them.
#[tauri::command]
pub async fn chat_attachment_open(
    app: AppHandle,
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let file = client
        .chat_attachment_file(&conversation_id, &attachment_id)
        .await?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| open_failure(format!("No se encontró la carpeta temporal: {error}")))?;
    let path = documents::stage(&cache, &attachment_id, file.name.as_deref(), &file.bytes)
        .map_err(|error| open_failure(format!("No se pudo guardar el archivo: {error}")))?;
    documents::open_with_system(&path)
        .map_err(|_| open_failure("No hay ninguna aplicación que pueda abrir este archivo.".into()))
}

/// Saves a document under the name the operator picks. `false` when the dialog was closed.
#[tauri::command]
pub async fn chat_attachment_save(
    app: AppHandle,
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
) -> Result<bool, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let file = client
        .chat_attachment_file(&conversation_id, &attachment_id)
        .await?;
    let name = file.name.unwrap_or_else(|| "documento".to_string());
    let Some(path) = chosen(|done| app.dialog().file().set_file_name(&name).save_file(done)).await
    else {
        return Ok(false);
    };
    std::fs::write(&path, &file.bytes)
        .map_err(|error| open_failure(format!("No se pudo guardar el archivo: {error}")))?;
    Ok(true)
}

fn open_failure(message: String) -> ChatFailure {
    ChatFailure::FileOpenFailed { message }
}
