//! Reading the chat: the home lists, search, filters and the loaded feed.

use herdr_desktop_core::chat::{
    ChatBootstrap, ChatCounts, ChatFailure, Conversation, ConversationDetail, ConversationPage,
    ListFilter, Section, CHAT_PAGE_LIMIT,
};
use herdr_desktop_core::ChatSnapshot;
use tauri::State;

use super::{client, shared, snapshot, Shared};
use crate::app_state::lock;

#[derive(serde::Serialize)]
pub struct ChatHome {
    bootstrap: ChatBootstrap,
    conversations: Vec<Conversation>,
    archived: Vec<Conversation>,
    sections: Vec<Section>,
}

#[derive(serde::Serialize)]
pub struct ChatOpen {
    conversation: Conversation,
    participants: Vec<herdr_desktop_core::chat::SeatSummary>,
    feed: ChatSnapshot,
}

#[tauri::command]
pub async fn chat_home(state: State<'_, Shared>) -> Result<ChatHome, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let (bootstrap, conversations, archived, sections) = tokio::join!(
        client.chat_bootstrap(),
        client.chat_conversations(None, None, false, false, None),
        client.chat_conversations(None, None, false, true, None),
        client.chat_sections(),
    );
    let conversations: ConversationPage = conversations?;
    let archived: ConversationPage = archived?;
    Ok(ChatHome {
        bootstrap: bootstrap?,
        conversations: conversations.conversations,
        archived: archived.conversations,
        sections: sections?,
    })
}

#[tauri::command]
pub async fn chat_search(
    state: State<'_, Shared>,
    q: String,
) -> Result<Vec<Conversation>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let query = q.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let page = client
        .chat_conversations(Some(query), None, false, false, None)
        .await?;
    Ok(page.conversations)
}

/// The «Pendientes de ti» and «Trabajando» chips: the host filters, the shell draws.
#[tauri::command]
pub async fn chat_filter_conversations(
    state: State<'_, Shared>,
    filter: String,
) -> Result<Vec<Conversation>, ChatFailure> {
    let filter = ListFilter::parse(&filter).ok_or_else(|| ChatFailure::Invalid {
        message: "Ese filtro no existe.".to_string(),
        field: Some("filter".to_string()),
    })?;
    let shared = shared(&state);
    let client = client(&shared)?;
    let page = client
        .chat_conversations(None, None, false, false, Some(filter))
        .await?;
    Ok(page.conversations)
}

/// `None` when the host predates the counts route: the shell hides the chips.
#[tauri::command]
pub async fn chat_counts(state: State<'_, Shared>) -> Result<Option<ChatCounts>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_counts().await
}

#[tauri::command]
pub async fn chat_open(
    state: State<'_, Shared>,
    conversation_id: String,
) -> Result<ChatOpen, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let (detail, messages, timeline, approvals) = tokio::join!(
        client.chat_conversation(&conversation_id),
        client.chat_messages(&conversation_id, None, None),
        client.chat_timeline(&conversation_id, None),
        client.chat_approvals(&conversation_id),
    );
    let detail: ConversationDetail = detail?;
    let messages = messages?;
    let timeline = timeline?;
    // The inbox is secondary: a device refused it still reads its chat.
    let approvals = approvals.unwrap_or_default();

    {
        let mut feed = lock(&shared.chat);
        feed.reset(&conversation_id);
        feed.merge_messages(messages.messages);
        feed.set_next_before(messages.next_before_seq);
        feed.set_last_read(detail.conversation.last_read_seq);
        feed.merge_timeline(timeline.entries);
        feed.set_approvals(approvals);
    }

    Ok(ChatOpen {
        conversation: detail.conversation,
        participants: detail.participants,
        feed: snapshot(&shared),
    })
}

#[tauri::command]
pub async fn chat_history(
    state: State<'_, Shared>,
    conversation_id: String,
    before_seq: i64,
) -> Result<ChatSnapshot, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let (messages, timeline) = tokio::join!(
        client.chat_messages(&conversation_id, None, Some(before_seq)),
        client.chat_timeline(&conversation_id, Some(before_seq)),
    );
    let messages = messages?;
    let timeline = timeline?;
    {
        let mut feed = lock(&shared.chat);
        feed.ensure(&conversation_id);
        feed.merge_messages(messages.messages);
        feed.set_next_before(messages.next_before_seq);
        feed.merge_timeline(timeline.entries);
    }
    Ok(snapshot(&shared))
}

/// Called from a `chat.*` frame, never from a timer: a reply that arrives on
/// the stream is shown with one signed read.
#[tauri::command]
pub async fn chat_refresh(
    state: State<'_, Shared>,
    conversation_id: String,
    after_seq: Option<i64>,
) -> Result<ChatSnapshot, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let current = lock(&shared.chat).last_seq();
    let after = after_seq.unwrap_or(current);

    if after <= 0 {
        let (messages, timeline, approvals) = tokio::join!(
            client.chat_messages(&conversation_id, None, None),
            client.chat_timeline(&conversation_id, None),
            client.chat_approvals(&conversation_id),
        );
        let messages = messages?;
        let timeline = timeline?;
        let approvals = approvals.unwrap_or_default();
        {
            let mut feed = lock(&shared.chat);
            feed.ensure(&conversation_id);
            feed.merge_messages(messages.messages);
            feed.set_next_before(messages.next_before_seq);
            feed.merge_timeline(timeline.entries);
            feed.set_approvals(approvals);
        }
        return Ok(snapshot(&shared));
    }

    let mut messages = Vec::new();
    let mut cursor = after;
    for _ in 0..20 {
        let page = client
            .chat_messages(&conversation_id, Some(cursor), None)
            .await?;
        let Some(last) = page.messages.last().map(|message| message.seq) else {
            break;
        };
        let short = page.messages.len() < usize::from(CHAT_PAGE_LIMIT);
        messages.extend(page.messages);
        cursor = last;
        if short {
            break;
        }
    }
    let (timeline, approvals) = tokio::join!(
        client.chat_timeline(&conversation_id, None),
        client.chat_approvals(&conversation_id),
    );
    let timeline = timeline?;
    let approvals = approvals.unwrap_or_default();
    {
        let mut feed = lock(&shared.chat);
        feed.ensure(&conversation_id);
        feed.merge_messages(messages);
        feed.merge_timeline(timeline.entries);
        feed.set_approvals(approvals);
    }
    Ok(snapshot(&shared))
}
