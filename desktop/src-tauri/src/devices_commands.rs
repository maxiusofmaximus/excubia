//! «Mis dispositivos» commands: the host's device list and the revoke of one
//! device. Each is a signed call that needs `ADMIN`; a refusal comes back as a
//! typed `DeviceFailure` the screen words by its `kind`.

use excubia_desktop_core::devices::{DeviceFailure, DeviceList};
use excubia_desktop_core::HostClient;
use tauri::State;

use crate::app_state::AppState;
use crate::chat_commands::{shared, Shared};

fn client(shared: &AppState) -> Result<HostClient, DeviceFailure> {
    shared
        .settings()
        .map_err(|_| DeviceFailure::Protocol)?
        .client()
        .map_err(DeviceFailure::from_core)
}

#[tauri::command]
pub async fn devices_list(state: State<'_, Shared>) -> Result<DeviceList, DeviceFailure> {
    let shared = shared(&state);
    client(&shared)?.devices().await
}

#[tauri::command]
pub async fn devices_revoke(state: State<'_, Shared>, id: String) -> Result<(), DeviceFailure> {
    let shared = shared(&state);
    client(&shared)?.device_revoke(&id).await
}

#[tauri::command]
pub async fn devices_grant(
    state: State<'_, Shared>,
    id: String,
    permissions: Vec<String>,
) -> Result<(), DeviceFailure> {
    let shared = shared(&state);
    client(&shared)?.device_grant(&id, &permissions).await
}
