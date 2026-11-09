use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use herdr_desktop_core::client::pair_device;
use herdr_desktop_core::state::{AgentRow, StateSnapshot};
use herdr_desktop_core::stream::ConnectionStatus;
use herdr_desktop_core::transport::is_loopback_url;
use herdr_desktop_core::{validate_device_label, CatalogAgent, CoreError, Transport, WriteAction};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::app_state::{lock, AppState, HostStatus};

type Shared = Arc<AppState>;

fn shared(state: &State<'_, Shared>) -> Shared {
    state.inner().clone()
}

#[derive(Serialize)]
pub struct Bootstrap {
    mode: String,
    host: HostStatus,
    host_url: String,
    device_id: String,
    label: String,
    fingerprint: Option<String>,
    paired: bool,
    notifications_enabled: bool,
    identity_error: Option<String>,
    state: Option<StateSnapshot>,
    connection: ConnectionStatus,
    /// The remote host's key is pinned (a LAN pairing link).
    pinned: bool,
    /// Clear HTTP to a host that is not on loopback: the UI must say so.
    unencrypted: bool,
    lan_pairing: bool,
}

#[derive(Serialize)]
pub struct PairOutcome {
    pub device_id: String,
    pub label: String,
    pub permissions: Vec<String>,
    pub recovery_code: Option<String>,
}

#[derive(Serialize)]
pub struct AgentDetail {
    agent: AgentRow,
    output: String,
    fetched_at: u64,
}

#[derive(Serialize)]
pub struct CreatedConversation {
    workspace_id: String,
    pane_id: String,
}

#[derive(Serialize)]
pub struct StopOutcome {
    already_gone: bool,
}

#[tauri::command]
pub async fn agent_catalog(state: State<'_, Shared>) -> Result<Vec<CatalogAgent>, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    client
        .catalog()
        .await
        .map_err(|error| error.describe(WriteAction::Read).text())
}

#[tauri::command]
pub async fn set_default_agent(state: State<'_, Shared>, name: String) -> Result<(), String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    client
        .set_default(&name)
        .await
        .map_err(|error| error.describe(WriteAction::SetDefault).text())
}

#[tauri::command]
pub async fn stop_agent(state: State<'_, Shared>, pane_id: String) -> Result<StopOutcome, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    match client.stop_agent(&pane_id).await {
        Ok(()) => Ok(StopOutcome {
            already_gone: false,
        }),
        Err(CoreError::Status {
            status: 400,
            code: Some(code),
            ..
        }) if code == "pane_gone" => Ok(StopOutcome { already_gone: true }),
        Err(error) => Err(error.describe(WriteAction::Stop).text()),
    }
}

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, Shared>,
    agent: String,
    cwd: String,
    label: Option<String>,
) -> Result<CreatedConversation, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    let created = client
        .create_workspace(&agent, &cwd, label.as_deref())
        .await
        .map_err(|error| error.describe(WriteAction::CreateWorkspace).text())?;
    Ok(CreatedConversation {
        workspace_id: created.workspace_id,
        pane_id: created.pane_id,
    })
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = sender.send(path);
    });
    let picked = receiver
        .await
        .map_err(|_| "el selector de carpetas se cerró".to_string())?;
    Ok(picked
        .and_then(|path| path.into_path().ok())
        .map(|path| path.display().to_string()))
}

#[tauri::command]
pub fn bootstrap(state: State<'_, Shared>) -> Bootstrap {
    let inner = lock(&state.inner);
    let identity = inner.identity.as_ref();
    let snapshot = lock(&inner.store).snapshot();
    let transport = inner.config.transport();
    Bootstrap {
        pinned: matches!(transport, Transport::Pinned(_)),
        unencrypted: matches!(transport, Transport::Plain)
            && !is_loopback_url(&inner.config.host_url),
        lan_pairing: inner.config.lan_pairing,
        mode: if inner.config.local {
            "local".to_string()
        } else {
            "remote".to_string()
        },
        host: inner.host.clone(),
        host_url: inner.config.host_url.clone(),
        device_id: identity
            .map(|identity| identity.device_id().to_string())
            .unwrap_or_default(),
        label: inner.config.label.clone(),
        fingerprint: identity.map(|identity| identity.fingerprint()),
        paired: inner.config.paired,
        notifications_enabled: inner.config.notifications,
        identity_error: inner.identity_error.clone(),
        state: snapshot,
        connection: inner.connection.clone(),
    }
}

#[tauri::command]
pub async fn respond(
    state: State<'_, Shared>,
    pane_id: String,
    text: String,
) -> Result<(), String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    client
        .respond(&pane_id, &text)
        .await
        .map_err(|error| error.describe(WriteAction::Respond).text())
}

#[tauri::command]
pub async fn send_keys(
    state: State<'_, Shared>,
    pane_id: String,
    keys: Vec<String>,
    confirm: bool,
) -> Result<(), String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    client
        .send_keys(&pane_id, &keys, confirm)
        .await
        .map_err(|error| error.describe(WriteAction::Keys).text())
}

#[tauri::command]
pub async fn mark_seen(state: State<'_, Shared>, pane_id: String) -> Result<(), String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    client
        .mark_seen(&pane_id)
        .await
        .map_err(|error| error.describe(WriteAction::Seen).text())
}

#[tauri::command]
pub fn set_notifications(state: State<'_, Shared>, enabled: bool) -> Result<(), String> {
    let shared = shared(&state);
    lock(&shared.inner).config.notifications = enabled;
    shared.save_config()
}

#[tauri::command]
pub async fn connect(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    shared(&state).start_stream(app)
}

#[tauri::command]
pub async fn pair(
    app: AppHandle,
    state: State<'_, Shared>,
    code: String,
    label: String,
) -> Result<PairOutcome, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let label = validate_device_label(&label).map_err(|error| error.to_string())?;
    let response = pair_device(
        &settings.url,
        &settings.transport,
        "p256",
        &settings.identity.public_key_b64(),
        &label,
        code.trim(),
    )
    .await
    .map_err(|error| error.to_string())?;

    let label = response.label.clone().unwrap_or(label);
    let mut identity = settings.identity;
    identity.set_device_id(response.device_id.clone());
    identity.set_label(label.clone());
    shared.update_identity(identity)?;
    {
        let mut inner = lock(&shared.inner);
        inner.config.device_id = Some(response.device_id.clone());
        inner.config.label = label.clone();
        inner.config.paired = true;
    }
    shared.save_config()?;
    shared.start_stream(app)?;

    Ok(PairOutcome {
        device_id: response.device_id,
        label,
        permissions: response.permissions,
        recovery_code: response.recovery_code.filter(|code| !code.is_empty()),
    })
}

#[tauri::command]
pub async fn refresh_state(
    app: AppHandle,
    state: State<'_, Shared>,
) -> Result<StateSnapshot, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let client = settings.client().map_err(|error| error.to_string())?;
    let snapshot = client.state().await.map_err(|error| error.to_string())?;

    let store = shared.store();
    if let Some(state) = lock(&store).apply(snapshot) {
        let _ = app.emit("hn-state", state.clone());
        return Ok(state);
    }
    let fallback = lock(&store).snapshot();
    fallback.ok_or_else(|| "No hay estado disponible todavía.".to_string())
}

#[tauri::command]
pub async fn agent_detail(
    state: State<'_, Shared>,
    pane_id: String,
    lines: u16,
) -> Result<AgentDetail, String> {
    let shared = shared(&state);
    let settings = shared.settings()?;
    let agent = lock(&shared.inner)
        .store
        .clone()
        .lock()
        .ok()
        .and_then(|store| store.snapshot())
        .and_then(|snapshot| {
            snapshot
                .agents
                .into_iter()
                .find(|agent| agent.pane_id == pane_id)
        })
        .ok_or_else(|| "Este agente ya no está en el estado actual.".to_string())?;

    let client = settings.client().map_err(|error| error.to_string())?;
    let output = client
        .read_output(&agent.pane_id, lines)
        .await
        .map_err(|error| error.describe(WriteAction::Read).text())?;

    Ok(AgentDetail {
        agent,
        output,
        fetched_at: unix_now(),
    })
}

#[tauri::command]
pub fn use_local_host(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let shared = shared(&state);
    {
        let mut inner = lock(&shared.inner);
        inner.config.local = true;
        inner.connection = ConnectionStatus::Idle;
    }
    shared.save_config()?;
    shared.stop_stream();
    lock(&shared.store()).clear();
    shared.start_local_host(app);
    Ok(())
}

#[tauri::command]
pub fn use_remote_host(state: State<'_, Shared>) -> Result<(), String> {
    let shared = shared(&state);
    shared.stop_local_host();
    shared.stop_stream();
    lock(&shared.store()).clear();
    {
        let mut inner = lock(&shared.inner);
        inner.config.local = false;
        inner.config.paired = false;
        inner.connection = ConnectionStatus::Idle;
    }
    shared.save_config()
}

#[tauri::command]
pub fn restart_host(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let shared = shared(&state);
    lock(&shared.store()).clear();
    shared.start_local_host(app);
    Ok(())
}

#[tauri::command]
pub fn forget_pairing(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let shared = shared(&state);
    shared.stop_stream();
    {
        let mut inner = lock(&shared.inner);
        inner.config.paired = false;
        inner.connection = ConnectionStatus::Idle;
    }
    lock(&shared.store()).clear();
    shared.save_config()?;
    let _ = app.emit("hn-connection", ConnectionStatus::Idle);
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
