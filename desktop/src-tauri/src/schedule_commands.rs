//! «Automatizar» commands: the scheduled tasks of the company, read and decided from the screen.
//! Each one is a signed call to the host; a refusal comes back as a typed `ChatFailure` the screen
//! words by its `code`. Nothing here runs a task on its own: every run is asked for.

use excubia_desktop_core::chat::{ChatFailure, Schedule, ScheduleDraft, ScheduleLaunch, ScheduleRun};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn schedules(state: State<'_, Shared>) -> Result<Vec<Schedule>, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedules().await
}

#[tauri::command]
pub async fn schedule_create(
    state: State<'_, Shared>,
    draft: ScheduleDraft,
) -> Result<Schedule, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_create(&draft).await
}

#[tauri::command]
pub async fn schedule_update(
    state: State<'_, Shared>,
    id: String,
    draft: ScheduleDraft,
) -> Result<Schedule, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_update(&id, &draft).await
}

#[tauri::command]
pub async fn schedule_delete(state: State<'_, Shared>, id: String) -> Result<String, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_delete(&id).await
}

#[tauri::command]
pub async fn schedule_set_paused(
    state: State<'_, Shared>,
    id: String,
    paused: bool,
) -> Result<Schedule, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_set_paused(&id, paused).await
}

#[tauri::command]
pub async fn schedule_run_now(
    state: State<'_, Shared>,
    id: String,
) -> Result<ScheduleLaunch, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_run_now(&id).await
}

#[tauri::command]
pub async fn schedule_runs(
    state: State<'_, Shared>,
    id: String,
) -> Result<Vec<ScheduleRun>, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.schedule_runs(&id).await
}
