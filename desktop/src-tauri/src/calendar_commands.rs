//! Calendar commands: the app's events and the company's zone. Each one is a signed
//! call to the host; a refusal comes back as a typed `ChatFailure` the screen words by
//! its `code`.

use herdr_desktop_core::chat::{
    CalendarEvent, ChatFailure, CompanyTimezone, EventDraft, EventPage,
};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn calendar_timezone(state: State<'_, Shared>) -> Result<CompanyTimezone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_timezone().await
}

#[tauri::command]
pub async fn calendar_set_timezone(
    state: State<'_, Shared>,
    timezone: String,
) -> Result<CompanyTimezone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_set_timezone(&timezone).await
}

#[tauri::command]
pub async fn calendar_events(
    state: State<'_, Shared>,
    from: String,
    to: String,
    after: Option<String>,
) -> Result<EventPage, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .calendar_events(&from, &to, after.as_deref())
        .await
}

#[tauri::command]
pub async fn calendar_get(
    state: State<'_, Shared>,
    id: String,
) -> Result<CalendarEvent, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_get(&id).await
}

#[tauri::command]
pub async fn calendar_create(
    state: State<'_, Shared>,
    draft: EventDraft,
) -> Result<CalendarEvent, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_create(&draft).await
}

#[tauri::command]
pub async fn calendar_update(
    state: State<'_, Shared>,
    id: String,
    draft: EventDraft,
) -> Result<CalendarEvent, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_update(&id, &draft).await
}

#[tauri::command]
pub async fn calendar_delete(state: State<'_, Shared>, id: String) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.calendar_delete(&id).await
}
