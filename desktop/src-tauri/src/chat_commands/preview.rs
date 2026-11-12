//! The page preview of a document. The host renders the page; the core checks the PNG and
//! its size before the webview gets the base64 copy.

use excubia_desktop_core::chat::{PreviewFailure, PreviewPage};
use tauri::State;

use super::{client, shared, Shared};

#[tauri::command]
pub async fn chat_attachment_preview(
    state: State<'_, Shared>,
    conversation_id: String,
    attachment_id: String,
    page: u32,
) -> Result<PreviewPage, PreviewFailure> {
    let shared = shared(&state);
    let client = client(&shared).map_err(|_| PreviewFailure::other())?;
    client
        .chat_attachment_preview(&conversation_id, &attachment_id, page)
        .await
}
