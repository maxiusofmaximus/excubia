//! «Plan propuesto»: the plans of the team, read and decided from the screen. Each command is
//! a signed call to the host; a refusal comes back as a typed `ChatFailure` the screen words by its `code`.

use excubia_desktop_core::chat::{ChatFailure, Plan};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn plans(state: State<'_, Shared>) -> Result<Vec<Plan>, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.plans().await
}

#[tauri::command]
pub async fn plan_approve(state: State<'_, Shared>, id: String) -> Result<Plan, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.plan_approve(&id).await
}

#[tauri::command]
pub async fn plan_reject(state: State<'_, Shared>, id: String) -> Result<Plan, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.plan_reject(&id).await
}

#[tauri::command]
pub async fn plan_request_changes(
    state: State<'_, Shared>,
    id: String,
    note: String,
) -> Result<Plan, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.plan_request_changes(&id, &note).await
}
