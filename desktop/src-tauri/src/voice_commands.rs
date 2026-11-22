//! Dictation: the webview sends the WAV as a raw body and gets the transcript back.
//! The bytes go straight to the host; nothing is written to disk or logged here.

use excubia_desktop_core::{VoiceFailure, VoiceTranscript};
use tauri::ipc::{InvokeBody, Request};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn voice_transcribe(
    state: State<'_, Shared>,
    request: Request<'_>,
) -> Result<VoiceTranscript, VoiceFailure> {
    let InvokeBody::Raw(wav) = request.body() else {
        return Err(VoiceFailure::Protocol);
    };
    let shared = shared(&state);
    let client = client(&shared).map_err(|_| VoiceFailure::Protocol)?;
    client.voice_transcribe(wav).await
}
