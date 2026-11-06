//! «Copias del turno»: the restore points of an agent. Each command is a signed call to
//! the host; a refusal comes back as a typed `ChatFailure` the screen words by its `code`.

use herdr_desktop_core::chat::{ChatFailure, RestoreChanges, RestoreDone, RestorePoints};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn restore_points(
    state: State<'_, Shared>,
    agent: String,
) -> Result<RestorePoints, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.restore_points(&agent).await
}

#[tauri::command]
pub async fn restore_changes(
    state: State<'_, Shared>,
    point: String,
) -> Result<RestoreChanges, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.restore_changes(&point).await
}

/// Puts the folder back as the point saw it. Also the undo: `point` is then the `undo_point`.
#[tauri::command]
pub async fn restore_point(
    state: State<'_, Shared>,
    point: String,
) -> Result<RestoreDone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.restore_point(&point).await
}
