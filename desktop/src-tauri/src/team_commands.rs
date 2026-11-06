//! Team commands: the role catalogue, the hires of this host and the first-run
//! onboarding. Each one is a signed call; a refusal comes back as a typed
//! `ChatFailure` whose `kind` the webview reads.

use herdr_desktop_core::chat::{
    ChatFailure, Hire, HireChange, Onboarding, OnboardingDone, RoleCatalog, TeamHires, TeamSummary,
};
use tauri::State;

use crate::chat_commands::{client, shared, Shared};

#[tauri::command]
pub async fn team_roles(state: State<'_, Shared>) -> Result<RoleCatalog, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_roles().await
}

#[tauri::command]
pub async fn team_hires(state: State<'_, Shared>) -> Result<TeamHires, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_hires().await
}

/// The status line of each hired professional. The screen polls it while Equipo is open.
#[tauri::command]
pub async fn team_summary(state: State<'_, Shared>) -> Result<TeamSummary, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_summary().await
}

#[tauri::command]
pub async fn team_hire(state: State<'_, Shared>, role: String) -> Result<Hire, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_hire(&role).await
}

#[tauri::command]
pub async fn team_hire_rename(
    state: State<'_, Shared>,
    hire_id: String,
    name: String,
) -> Result<Hire, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .team_hire_change(&hire_id, &HireChange::Name(name))
        .await
}

/// `avatar: null` removes the character; the initial stands in for it.
#[tauri::command]
pub async fn team_hire_avatar(
    state: State<'_, Shared>,
    hire_id: String,
    avatar: Option<String>,
) -> Result<Hire, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .team_hire_change(&hire_id, &HireChange::Avatar(avatar))
        .await
}

#[tauri::command]
pub async fn team_hire_dismiss(
    state: State<'_, Shared>,
    hire_id: String,
) -> Result<Hire, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_hire_dismiss(&hire_id).await
}

#[tauri::command]
pub async fn team_onboarding(state: State<'_, Shared>) -> Result<Onboarding, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_onboarding().await
}

#[tauri::command]
pub async fn team_onboarding_skip(state: State<'_, Shared>) -> Result<OnboardingDone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.team_onboarding_skip().await
}

#[tauri::command]
pub async fn team_onboarding_contract(
    state: State<'_, Shared>,
    areas: Vec<String>,
    roles: Vec<String>,
) -> Result<OnboardingDone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?
        .team_onboarding_contract(&areas, &roles)
        .await
}
