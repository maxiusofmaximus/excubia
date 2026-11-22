//! Delegated work: a delegation's detail, cancelling it, and stopping a
//! conversation's running turn.

use excubia_desktop_core::chat::{
    delegation_text, CancelledConversation, ChatFailure, DelegationDetail, StopOutcome,
};
use tauri::State;

use super::{client, shared, Shared};

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
pub async fn chat_stop_conversation(
    state: State<'_, Shared>,
    conversation_id: String,
) -> Result<StopOutcome, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_stop_conversation(&conversation_id).await
}
