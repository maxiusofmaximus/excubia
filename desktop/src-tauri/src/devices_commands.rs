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
    // One snapshot for identity, endpoint and mode, so the guard sees the mode the request uses.
    let settings = shared.settings().map_err(|_| DeviceFailure::Protocol)?;
    let client = settings.client().map_err(DeviceFailure::from_core)?;
    // Checked before any request: in local mode this device is the server itself.
    if refuses_own_server(settings.local, &id, client.device_id()) {
        return Err(DeviceFailure::OwnServer);
    }
    client.device_revoke(&id).await
}

fn refuses_own_server(local: bool, id: &str, own: &str) -> bool {
    local && id.trim() == own
}

#[cfg(test)]
mod tests {
    use super::refuses_own_server;

    #[test]
    fn the_local_server_never_revokes_its_own_device() {
        assert!(refuses_own_server(true, "dev-a", "dev-a"));
        assert!(refuses_own_server(true, " dev-a ", "dev-a"));
        assert!(!refuses_own_server(true, "dev-b", "dev-a"));
        assert!(!refuses_own_server(false, "dev-a", "dev-a"));
    }
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
