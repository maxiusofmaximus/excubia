//! Chat history vault: the state, the key creation from mouse samples and the
//! removal of the plain backup. Only an admin device reaches these routes.

use excubia_desktop_core::chat::{ChatFailure, VaultDone, VaultInit, VaultStatus};
use tauri::State;

use super::{client, shared, Shared};

#[tauri::command]
pub async fn vault_status(state: State<'_, Shared>) -> Result<VaultStatus, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.vault_status().await
}

/// Creates the key from the packed samples (base64). The phrase comes back once, for the
/// recovery screen; it is never logged.
#[tauri::command]
pub async fn vault_init(
    state: State<'_, Shared>,
    samples: String,
) -> Result<VaultInit, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.vault_init(&samples).await
}

#[tauri::command]
pub async fn vault_finish(state: State<'_, Shared>) -> Result<VaultDone, ChatFailure> {
    let shared = shared(&state);
    client(&shared)?.vault_finish().await
}
