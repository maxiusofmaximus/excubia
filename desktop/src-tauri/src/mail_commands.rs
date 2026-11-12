//! Mail commands: the accounts, the device-code sign-in, the draft and the send
//! the operator approved. Each one is a signed call; a refusal comes back as a
//! `MailFailure` whose code the webview reads and whose message is already Spanish.

use excubia_desktop_core::mail::{
    DeviceLogin, LoginStatus, MailAccount, MailDraft, MailFailure, MailResult, SendRequest,
    SendState, SendToggled,
};
use excubia_desktop_core::HostClient;
use tauri::State;

use crate::app_state::AppState;
use crate::chat_commands::{shared, Shared};

fn client(shared: &AppState) -> MailResult<HostClient> {
    shared
        .settings()
        .ok()
        .and_then(|settings| settings.client().ok())
        .ok_or_else(|| MailFailure::coded("local_setup", None))
}

#[tauri::command]
pub async fn mail_accounts(state: State<'_, Shared>) -> Result<Vec<MailAccount>, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_accounts().await
}

#[tauri::command]
pub async fn mail_login_start(
    state: State<'_, Shared>,
    account: String,
    tenant: String,
) -> Result<DeviceLogin, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_login_start(&account, &tenant).await
}

#[tauri::command]
pub async fn mail_login_status(
    state: State<'_, Shared>,
    account: String,
) -> Result<LoginStatus, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_login_status(&account).await
}

#[tauri::command]
pub async fn mail_send_enable(
    state: State<'_, Shared>,
    account: String,
) -> Result<DeviceLogin, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_send_enable(&account).await
}

#[tauri::command]
pub async fn mail_send_enable_status(
    state: State<'_, Shared>,
    account: String,
) -> Result<LoginStatus, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_send_enable_status(&account).await
}

#[tauri::command]
pub async fn mail_send_disable(
    state: State<'_, Shared>,
    account: String,
) -> Result<SendToggled, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_send_disable(&account).await
}

#[tauri::command]
pub async fn mail_draft(
    state: State<'_, Shared>,
    account: String,
    draft_id: String,
) -> Result<MailDraft, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_draft(&account, &draft_id).await
}

#[tauri::command]
pub async fn mail_draft_update(
    state: State<'_, Shared>,
    account: String,
    draft_id: String,
    subject: String,
    body: String,
) -> Result<MailDraft, MailFailure> {
    let shared = shared(&state);
    client(&shared)?
        .mail_draft_update(&account, &draft_id, &subject, &body)
        .await
}

#[tauri::command]
pub async fn mail_draft_send(
    state: State<'_, Shared>,
    account: String,
    draft_id: String,
    approval_id: Option<String>,
) -> Result<SendRequest, MailFailure> {
    let shared = shared(&state);
    client(&shared)?
        .mail_draft_send(&account, &draft_id, approval_id.as_deref())
        .await
}

#[tauri::command]
pub async fn mail_send_state(
    state: State<'_, Shared>,
    send_id: String,
) -> Result<SendState, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_send_state(&send_id).await
}

#[tauri::command]
pub async fn mail_send_cancel(
    state: State<'_, Shared>,
    send_id: String,
) -> Result<SendState, MailFailure> {
    let shared = shared(&state);
    client(&shared)?.mail_send_cancel(&send_id).await
}
