//! The operator's approvals: the inbox, resolved history, decisions and the
//! approval mode settings.

use excubia_desktop_core::chat::{Approval, ApprovalModeChange, ApprovalSettings, ChatFailure};
use tauri::State;

use super::{client, shared, Shared};
use crate::app_state::lock;

/// The operator's answer to one approval. The feed takes the host's record at
/// once, so the card shows the settled state before the next refresh. `exclude`
/// leaves mails out of a `mail.move` batch; without it the decision is the same as before.
#[tauri::command]
pub async fn chat_approval_decision(
    state: State<'_, Shared>,
    approval_id: String,
    decision: String,
    client_id: String,
    exclude: Option<Vec<String>>,
) -> Result<Approval, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    let approval = client
        .chat_approval_decide(
            &approval_id,
            &decision,
            &client_id,
            &exclude.unwrap_or_default(),
        )
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

/// One approval by id, from the inbox or a resolved status. A mail send follows
/// it until the operator decides.
#[tauri::command]
pub async fn chat_approval_find(
    state: State<'_, Shared>,
    approval_id: String,
) -> Result<Option<Approval>, ChatFailure> {
    let shared = shared(&state);
    let client = client(&shared)?;
    client.chat_approval_find(&approval_id).await
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
