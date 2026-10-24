//! Chat commands: the screen reads a reduced snapshot, never raw pages.
//!
//! Every command signs with the device identity and returns a typed
//! `ChatFailure`, so the webview picks a card by `kind` instead of parsing
//! status codes.

use std::sync::Arc;

use herdr_desktop_core::chat::{
    delegation_text, Approval, ApprovalModeChange, ApprovalSettings, CancelledConversation,
    ChatBootstrap, ChatFailure, ChatResult, Conversation, ConversationDetail, ConversationPage,
    ConversationPatch, DelegationDetail, Message, Section, SectionChange, SendAccepted, Turn,
    CHAT_PAGE_LIMIT,
};
use herdr_desktop_core::{ChatSnapshot, GatewayClient};
use tauri::State;

use crate::app_state::{lock, AppState};

type Shared = Arc<AppState>;

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

#[derive(serde::Serialize)]
pub struct ChatSend {
    message: Message,
    turn: Turn,
    feed: ChatSnapshot,
}

#[derive(serde::Serialize)]
pub struct DelegationView {
    detail: DelegationDetail,
    reason_text: Option<String>,
}

fn delegation_view(detail: DelegationDetail) -> DelegationView {
    let reason_text = detail.delegation.reason.as_deref().map(|reason| {
        delegation_text(reason)
            .map(str::to_string)
            .unwrap_or_else(|| reason.to_string())
    });
    DelegationView {
        detail,
        reason_text,
    }
}

fn shared(state: &State<'_, Shared>) -> Shared {
    state.inner().clone()
}

fn client(shared: &AppState) -> ChatResult<GatewayClient> {
    let settings = shared.settings().map_err(|message| ChatFailure::Protocol {
        message: format!("La configuración local no está lista: {message}"),
    })?;
    settings.client().map_err(|error| ChatFailure::Protocol {
        message: format!("No se pudo preparar la conexión: {error}"),
    })
}

fn snapshot(shared: &AppState) -> ChatSnapshot {
    lock(&shared.chat).snapshot()
}

#[tauri::command]
pub async fn chat_home(state: State<'_, Shared>) -> Result<ChatHome, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let (bootstrap, conversations, archived, sections) = tokio::join!(
        client.chat_bootstrap(),
        client.chat_conversations(None, None, false, false),
        client.chat_conversations(None, None, false, true),
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
        .chat_conversations(Some(query), None, false, false)
        .await?;
    Ok(page.conversations)
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

/// The operator's answer to one approval. The feed takes the host's record at
/// once, so the card shows the settled state before the next refresh.
#[tauri::command]
pub async fn chat_approval_decision(
    state: State<'_, Shared>,
    approval_id: String,
    decision: String,
    client_id: String,
) -> Result<Approval, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let approval = client
        .chat_approval_decide(&approval_id, &decision, &client_id)
        .await?;
    lock(&shared.chat).replace_approval(approval.clone());
    Ok(approval)
}

/// The operator's inbox: every approval that waits for him, across
/// conversations. Conversation-less ones only show up here.
#[tauri::command]
pub async fn chat_pending_approvals(
    state: State<'_, Shared>,
) -> Result<Vec<Approval>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_pending_approvals().await
}

/// The host's approvals of one resolved status. An approval that left the inbox
/// is found here: approved, denied, expired or consumed.
#[tauri::command]
pub async fn chat_approvals_with_status(
    state: State<'_, Shared>,
    status: String,
) -> Result<Vec<Approval>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_approvals_with_status(&status).await
}

#[derive(serde::Serialize)]
pub struct ApprovalSettingsView {
    #[serde(flatten)]
    settings: ApprovalSettings,
    /// The device the operator grants `APPROVE` to, from the local identity.
    device_id: String,
}

#[tauri::command]
pub async fn chat_approval_settings(
    state: State<'_, Shared>,
) -> Result<ApprovalSettingsView, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let settings = client.chat_approval_settings().await?;
    Ok(ApprovalSettingsView {
        settings,
        device_id: client.device_id().to_string(),
    })
}

#[tauri::command]
pub async fn chat_set_approval_mode(
    state: State<'_, Shared>,
    client_id: String,
    mode: String,
    project_id: Option<String>,
) -> Result<ApprovalModeChange, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client
        .chat_set_approval_mode(&client_id, &mode, project_id.as_deref())
        .await
}

#[tauri::command]
pub async fn chat_send(
    state: State<'_, Shared>,
    conversation_id: String,
    client_id: String,
    text: String,
) -> Result<ChatSend, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let accepted: SendAccepted = client
        .chat_send(&conversation_id, &client_id, &text)
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
pub async fn chat_delegation(
    state: State<'_, Shared>,
    delegation_id: String,
) -> Result<DelegationView, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    Ok(delegation_view(
        client.chat_delegation(&delegation_id).await?,
    ))
}

#[tauri::command]
pub async fn chat_cancel_delegation(
    state: State<'_, Shared>,
    delegation_id: String,
) -> Result<DelegationView, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    Ok(delegation_view(
        client.chat_cancel_delegation(&delegation_id).await?,
    ))
}

#[tauri::command]
pub async fn chat_cancel_conversation(
    state: State<'_, Shared>,
    conversation_id: String,
) -> Result<CancelledConversation, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_cancel_conversation(&conversation_id).await
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
