//! Chat writes: sending, marking as read, creating and editing conversations
//! and their sections.

use herdr_desktop_core::chat::{
    ChatFailure, Conversation, ConversationPatch, Message, Section, SectionChange, SendAccepted,
    Turn,
};
use herdr_desktop_core::ChatSnapshot;
use tauri::State;

use super::{client, shared, snapshot, Shared};
use crate::app_state::lock;

#[derive(serde::Serialize)]
pub struct ChatSend {
    message: Message,
    turn: Turn,
    feed: ChatSnapshot,
}

#[tauri::command]
pub async fn chat_send(
    state: State<'_, Shared>,
    conversation_id: String,
    client_id: String,
    text: String,
    reply_to: Option<String>,
    attachments: Option<Vec<String>>,
) -> Result<ChatSend, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let accepted: SendAccepted = client
        .chat_send_message(
            &conversation_id,
            &client_id,
            &text,
            reply_to.as_deref(),
            &attachments.unwrap_or_default(),
        )
        .await?;
    {
        let mut feed = lock(&shared.chat);
        feed.ensure(&conversation_id);
        feed.merge_messages([accepted.message.clone()]);
    }
    Ok(ChatSend {
        message: accepted.message,
        turn: accepted.turn,
        feed: snapshot(&shared),
    })
}

#[tauri::command]
pub async fn chat_mark_read(
    state: State<'_, Shared>,
    conversation_id: String,
    last_seq: i64,
) -> Result<i64, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let read = client.chat_read(&conversation_id, last_seq).await?;
    let mut feed = lock(&shared.chat);
    if feed.conversation_id() == Some(conversation_id.as_str()) {
        feed.set_last_read(read.last_read_seq);
    }
    Ok(read.last_read_seq)
}

#[tauri::command]
pub async fn chat_create(
    state: State<'_, Shared>,
    title: Option<String>,
) -> Result<Conversation, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let title = title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    client.chat_create_conversation(title).await
}

#[tauri::command]
pub async fn chat_update(
    state: State<'_, Shared>,
    conversation_id: String,
    title: Option<String>,
    pinned: Option<bool>,
    section_id: Option<String>,
    clear_section: Option<bool>,
    archived: Option<bool>,
) -> Result<Conversation, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let section = match (section_id, clear_section.unwrap_or(false)) {
        (Some(section_id), _) => SectionChange::Set(section_id),
        (None, true) => SectionChange::Clear,
        (None, false) => SectionChange::Keep,
    };
    let patch = ConversationPatch {
        title,
        pinned,
        section,
        archived,
    };
    client
        .chat_update_conversation(&conversation_id, &patch)
        .await
}

#[tauri::command]
pub async fn chat_save_section(
    state: State<'_, Shared>,
    id: Option<String>,
    name: String,
) -> Result<Section, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_save_section(id.as_deref(), &name).await
}
