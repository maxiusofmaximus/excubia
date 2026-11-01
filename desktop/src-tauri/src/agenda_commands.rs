//! Agenda commands: the app's notes and contacts. Each one is a signed call to
//! the host; a refusal comes back as a typed `ChatFailure` the screen words by
//! its `code`.

use herdr_desktop_core::chat::{
    ChatFailure, Contact, ContactDraft, ContactPage, Note, NoteDraft, NotePage,
};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn agenda_notes(
    state: State<'_, Shared>,
    q: Option<String>,
    before: Option<String>,
) -> Result<NotePage, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .notes(q.as_deref(), before.as_deref())
        .await
}

#[tauri::command]
pub async fn agenda_note(state: State<'_, Shared>, id: String) -> Result<Note, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.note(&id).await
}

#[tauri::command]
pub async fn agenda_note_create(
    state: State<'_, Shared>,
    draft: NoteDraft,
) -> Result<Note, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.note_create(&draft).await
}

/// `title: Some("")` clears the subject; `None` keeps it.
#[tauri::command]
pub async fn agenda_note_update(
    state: State<'_, Shared>,
    id: String,
    title: Option<String>,
    body: String,
) -> Result<Note, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .note_update(&id, title.as_deref(), &body)
        .await
}

#[tauri::command]
pub async fn agenda_note_delete(state: State<'_, Shared>, id: String) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.note_delete(&id).await
}

#[tauri::command]
pub async fn agenda_contacts(
    state: State<'_, Shared>,
    q: Option<String>,
    after: Option<String>,
) -> Result<ContactPage, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .contacts(q.as_deref(), after.as_deref())
        .await
}

#[tauri::command]
pub async fn agenda_contact(state: State<'_, Shared>, id: String) -> Result<Contact, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.contact(&id).await
}

#[tauri::command]
pub async fn agenda_contact_create(
    state: State<'_, Shared>,
    draft: ContactDraft,
) -> Result<Contact, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.contact_create(&draft).await
}

#[tauri::command]
pub async fn agenda_contact_update(
    state: State<'_, Shared>,
    id: String,
    draft: ContactDraft,
) -> Result<Contact, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.contact_update(&id, &draft).await
}

#[tauri::command]
pub async fn agenda_contact_delete(
    state: State<'_, Shared>,
    id: String,
) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.contact_delete(&id).await
}
