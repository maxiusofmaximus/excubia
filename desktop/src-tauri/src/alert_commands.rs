//! The alert rules of «Avisos» and the mode they run in. Each one is a signed call; a refusal
//! comes back as a typed `ChatFailure` the webview reads.

use herdr_desktop_core::chat::{AlertMode, AlertRule, AlertRuleDraft, ChatFailure};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn alert_rules(state: State<'_, Shared>) -> Result<Vec<AlertRule>, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_rules().await
}

#[tauri::command]
pub async fn alert_rule_create(
    state: State<'_, Shared>,
    draft: AlertRuleDraft,
) -> Result<AlertRule, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_rule_create(&draft).await
}

#[tauri::command]
pub async fn alert_rule_update(
    state: State<'_, Shared>,
    id: String,
    enabled: bool,
) -> Result<AlertRule, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_rule_set_enabled(&id, enabled).await
}

#[tauri::command]
pub async fn alert_rule_delete(state: State<'_, Shared>, id: String) -> Result<(), ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_rule_delete(&id).await
}

#[tauri::command]
pub async fn alert_mode(state: State<'_, Shared>) -> Result<AlertMode, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_mode().await
}

#[tauri::command]
pub async fn alert_mode_set(
    state: State<'_, Shared>,
    mode: AlertMode,
) -> Result<AlertMode, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.alert_mode_set(mode).await
}
