use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use herdr_desktop_core::chat::ChatEvent;
use herdr_desktop_core::client::pair_device;
use herdr_desktop_core::identity::{DeviceIdentity, IdentityStore, KeyringIdentityStore};
use herdr_desktop_core::state::{StateSnapshot, StateStore};
use herdr_desktop_core::stream::{run_event_loop, ConnectionStatus, EventSink, StreamOptions};
use herdr_desktop_core::{
    ChatFeed, CoreError, GatewayClient, TransitionTracker, Transport, WriteAction,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::host_process::{self, HostConfig, HostEvent};
use crate::lan;

const KEYRING_SERVICE: &str = "dev.herdrnotify.desktop";
const KEYRING_ACCOUNT: &str = "device-identity-v1";
const LOCAL_HOST_LABEL: &str = "herdr notify";

/// What the local core is doing, for the UI to render.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum HostStatus {
    Disabled,
    Starting,
    Ready { port: u16 },
    Failed { error: String },
}

pub struct Inner {
    pub config: Config,
    pub identity: Option<DeviceIdentity>,
    pub identity_error: Option<String>,
    pub store: Arc<Mutex<StateStore>>,
    pub connection: ConnectionStatus,
    /// The URL actually in use: the managed host's port when local.
    pub runtime_url: Option<String>,
    pub host: HostStatus,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
    pub config_path: PathBuf,
    /// The loaded conversation, reduced by `seq`. Reset when another chat is
    /// opened; the host remains the source of truth.
    pub chat: Mutex<ChatFeed>,
    cancel: Mutex<Option<CancellationToken>>,
    host_config: Mutex<Option<HostConfig>>,
    host_shutdown: Mutex<Option<CancellationToken>>,
    host_tasks: Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
    keyring: KeyringIdentityStore,
}

/// A poisoned lock is not a reason to take the app down: the guarded data is
/// still usable, and every accessor here recovers instead of panicking.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything a signed request needs, read once per command.
pub struct GatewaySettings {
    pub url: String,
    pub identity: DeviceIdentity,
    pub transport: Transport,
}

impl GatewaySettings {
    pub fn client(&self) -> Result<GatewayClient, CoreError> {
        GatewayClient::with_transport(&self.url, self.identity.clone(), &self.transport)
    }
}

impl AppState {
    pub fn new(app: &AppHandle) -> Self {
        let config_path = app
            .path()
            .app_config_dir()
            .map(|directory| directory.join("config.json"))
            .unwrap_or_else(|_| PathBuf::from("desktop-config.json"));
        let mut config = Config::load(&config_path);
        let keyring = KeyringIdentityStore::new(KEYRING_SERVICE, KEYRING_ACCOUNT);
        let (identity, identity_error) = load_identity(&keyring, &mut config);
        if let Err(error) = config.save(&config_path) {
            eprintln!("no se pudo guardar la configuración inicial: {error}");
        }

        Self {
            inner: Mutex::new(Inner {
                config,
                identity,
                identity_error,
                store: Arc::new(Mutex::new(StateStore::default())),
                connection: ConnectionStatus::Idle,
                runtime_url: None,
                host: HostStatus::Disabled,
            }),
            config_path,
            chat: Mutex::new(ChatFeed::default()),
            cancel: Mutex::new(None),
            host_config: Mutex::new(None),
            host_shutdown: Mutex::new(None),
            host_tasks: Mutex::new(Vec::new()),
            keyring,
        }
    }

    pub fn save_config(&self) -> Result<(), String> {
        let inner = lock(&self.inner);
        inner
            .config
            .save(&self.config_path)
            .map_err(|error| error.to_string())
    }

    pub fn settings(&self) -> Result<GatewaySettings, String> {
        let inner = lock(&self.inner);
        let identity = inner
            .identity
            .clone()
            .ok_or_else(|| "No hay identidad de dispositivo disponible.".to_string())?;
        let url = inner
            .runtime_url
            .clone()
            .unwrap_or_else(|| inner.config.gateway_url.clone());
        Ok(GatewaySettings {
            url,
            identity,
            transport: inner.config.transport(),
        })
    }

    pub fn store(&self) -> Arc<Mutex<StateStore>> {
        lock(&self.inner).store.clone()
    }

    /// Persists the identity before swapping it in memory, so a failed keyring
    /// write cannot leave the app signing with a key the store does not have.
    pub fn update_identity(&self, identity: DeviceIdentity) -> Result<(), String> {
        self.keyring
            .save(&identity)
            .map_err(|error| error.to_string())?;
        lock(&self.inner).identity = Some(identity);
        Ok(())
    }

    pub fn stop_stream(&self) {
        if let Some(cancel) = lock(&self.cancel).take() {
            cancel.cancel();
        }
    }

    pub fn start_stream(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        let local_ready = {
            let inner = lock(&self.inner);
            !inner.config.local || inner.runtime_url.is_some()
        };
        if !local_ready {
            return Err("el host local todavía está arrancando".to_string());
        }
        let client = self
            .settings()?
            .client()
            .map_err(|error| error.to_string())?;
        let cancel = CancellationToken::new();
        if let Some(previous) = lock(&self.cancel).replace(cancel.clone()) {
            previous.cancel();
        }

        let store = self.store();
        let sink = Arc::new(TauriSink {
            app,
            shared: self.clone(),
            tracker: Mutex::new(TransitionTracker::default()),
        });
        tauri::async_runtime::spawn(async move {
            run_event_loop(
                Arc::new(client),
                store,
                sink,
                cancel,
                StreamOptions::default(),
            )
            .await;
        });
        Ok(())
    }

    /// Spawns the bundled core and, once it is listening, pairs this device
    /// automatically and connects. The user never sees a code in local mode.
    pub fn start_local_host(self: &Arc<Self>, app: AppHandle) {
        self.stop_local_host();

        let lan_bind = lock(&self.inner)
            .config
            .lan_pairing
            .then(lan::lan_bind)
            .flatten();
        let host_config = match host_config_for(&app, lan_bind) {
            Ok(config) => config,
            Err(error) => return self.fail_host(&app, error),
        };
        *lock(&self.host_config) = Some(host_config.clone());
        lock(&self.inner).host = HostStatus::Starting;
        let _ = app.emit("hn-host", HostStatus::Starting);

        let shutdown = CancellationToken::new();
        *lock(&self.host_shutdown) = Some(shutdown.clone());

        let (events, mut received) = mpsc::unbounded_channel();
        // The Tauri runtime is the one that exists this early; spawning here
        // (not with tokio::spawn) is what makes `setup` safe.
        let supervisor = tauri::async_runtime::spawn(host_process::supervise(
            host_config,
            events,
            shutdown.clone(),
        ));
        let shared = self.clone();
        let orchestrator = tauri::async_runtime::spawn(async move {
            loop {
                let event = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    event = received.recv() => match event {
                        Some(event) => event,
                        None => break,
                    },
                };
                match event {
                    HostEvent::Ready { port } => {
                        let url = format!("http://127.0.0.1:{port}");
                        {
                            let mut inner = lock(&shared.inner);
                            inner.host = HostStatus::Ready { port };
                            inner.runtime_url = Some(url.clone());
                        }
                        let _ = app.emit("hn-host", HostStatus::Ready { port });
                        if let Err(error) = shared.ensure_local_pairing(&app, &url).await {
                            shared.fail_host(&app, error);
                            continue;
                        }
                        if let Err(error) = shared.start_stream(app.clone()) {
                            shared.fail_host(&app, error);
                        }
                    }
                    HostEvent::Failed { error } => shared.fail_host(&app, error),
                    HostEvent::Started { .. }
                    | HostEvent::Exited { .. }
                    | HostEvent::Restarting { .. } => {
                        lock(&shared.inner).host = HostStatus::Starting;
                        let _ = app.emit("hn-host", HostStatus::Starting);
                    }
                }
            }
        });

        *lock(&self.host_tasks) = vec![supervisor, orchestrator];
    }

    pub fn stop_local_host(&self) {
        if let Some(shutdown) = lock(&self.host_shutdown).take() {
            shutdown.cancel();
        }
        for task in lock(&self.host_tasks).drain(..) {
            task.abort();
        }
        *lock(&self.host_config) = None;
        let mut inner = lock(&self.inner);
        inner.runtime_url = None;
        if inner.host != HostStatus::Disabled {
            inner.host = HostStatus::Disabled;
        }
    }

    fn fail_host(&self, app: &AppHandle, error: String) {
        let status = HostStatus::Failed {
            error: error.clone(),
        };
        lock(&self.inner).host = status.clone();
        let _ = app.emit("hn-host", status);
    }

    /// Pairs this device against the managed host if it does not know it yet,
    /// then grants the permissions the UI needs. Both are host-side acts and
    /// the app is the host here.
    async fn ensure_local_pairing(
        self: &Arc<Self>,
        app: &AppHandle,
        base_url: &str,
    ) -> Result<(), String> {
        let identity = lock(&self.inner)
            .identity
            .clone()
            .ok_or_else(|| "No hay identidad de dispositivo disponible.".to_string())?;
        let client =
            GatewayClient::new(base_url, identity.clone()).map_err(|error| error.to_string())?;
        match client.state().await {
            Ok(state) => {
                if let Some(state) = lock(&self.store()).apply(state) {
                    let _ = app.emit("hn-state", state);
                }
                return Ok(());
            }
            Err(CoreError::Unauthorized) => {}
            Err(error) => return Err(error.describe(WriteAction::Read).text()),
        }

        let code = pair_code_from(&self.run_host_cli(&["pair-code"]).await?)?;
        let label = lock(&self.inner).config.label.clone();
        let response = pair_device(
            base_url,
            &Transport::Plain,
            "p256",
            &identity.public_key_b64(),
            &label,
            &code,
        )
        .await
        .map_err(|error| error.to_string())?;

        let mut updated = identity;
        updated.set_device_id(response.device_id.clone());
        updated.set_label(label);
        self.update_identity(updated)?;
        {
            let mut inner = lock(&self.inner);
            inner.config.device_id = Some(response.device_id.clone());
            inner.config.paired = true;
        }
        self.save_config()?;

        for permission in ["respond", "send_key"] {
            self.run_host_cli(&[
                "devices",
                "grant",
                &response.device_id,
                "--permission",
                permission,
            ])
            .await?;
        }
        Ok(())
    }

    /// Runs the host CLI against the same data dir the daemon uses.
    pub async fn run_host_cli(&self, args: &[&str]) -> Result<String, String> {
        let config = lock(&self.host_config)
            .clone()
            .ok_or_else(|| "el host local no está configurado".to_string())?;
        let output = tokio::process::Command::new(&config.binary)
            .args(args)
            .arg("--data-dir")
            .arg(&config.data_dir)
            .output()
            .await
            .map_err(|error| format!("no se pudo ejecutar el host: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "host {}: {}",
                args.first().copied().unwrap_or(""),
                stderr.trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// `pairing code: ABC123` → `ABC123`.
pub fn pair_code_from(stdout: &str) -> Result<String, String> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("pairing code: "))
        .map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
        .ok_or_else(|| "el host no devolvió un código de emparejamiento".to_string())
}

/// The app-owned data directory the local host (and its `agents.toml`) uses.
pub fn host_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("core"))
        .map_err(|error| error.to_string())
}

/// The sidecar plus the app-owned data and log directories.
fn host_config_for(app: &AppHandle, lan_bind: Option<SocketAddr>) -> Result<HostConfig, String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
        .ok_or_else(|| "no pude resolver el directorio de la aplicación".to_string())?;
    let binary = host_process::host_binary_path(&exe_dir).ok_or_else(|| {
        format!(
            "no encuentro el núcleo local en {}; ejecuta scripts/build-host.ps1",
            exe_dir.display()
        )
    })?;
    let data_dir = host_data_dir(app)?;
    crate::agents_seed::seed_for_env(&data_dir);
    let log_path = app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?
        .join("host.log");
    let mut args = vec![
        "--port".to_string(),
        "0".to_string(),
        "--label".to_string(),
        LOCAL_HOST_LABEL.to_string(),
        "--data-dir".to_string(),
        data_dir.display().to_string(),
    ];
    if let Some(address) = lan_bind {
        args.push("--tls-bind".to_string());
        args.push(address.to_string());
    }
    if let Ok(value) = std::env::var("HN_CORE_SPAWN") {
        for spawn in value
            .split(';')
            .map(str::trim)
            .filter(|spawn| !spawn.is_empty())
        {
            args.push("--spawn".to_string());
            args.push(spawn.to_string());
        }
    }
    Ok(HostConfig {
        binary,
        args,
        data_dir,
        log_path,
    })
}

fn load_identity(
    keyring: &KeyringIdentityStore,
    config: &mut Config,
) -> (Option<DeviceIdentity>, Option<String>) {
    match DeviceIdentity::load_or_create(
        keyring,
        config.device_id.as_deref().unwrap_or(""),
        &config.label,
    ) {
        Ok(mut identity) => {
            if let Some(device_id) = config
                .device_id
                .as_deref()
                .filter(|device_id| !device_id.trim().is_empty())
            {
                identity.set_device_id(device_id);
            }
            identity.set_label(config.label.clone());
            config.device_id = Some(identity.device_id().to_string());
            if let Err(error) = keyring.save(&identity) {
                eprintln!("no se pudo normalizar la identidad en el almacén seguro: {error}");
            }
            (Some(identity), None)
        }
        Err(error) => (None, Some(error.to_string())),
    }
}

struct TauriSink {
    app: AppHandle,
    shared: Arc<AppState>,
    tracker: Mutex<TransitionTracker>,
}

#[async_trait]
impl EventSink for TauriSink {
    async fn connection(&self, status: ConnectionStatus) {
        lock(&self.shared.inner).connection = status.clone();
        let _ = self.app.emit("hn-connection", status);
    }

    async fn state(&self, state: StateSnapshot) {
        let alerts = lock(&self.tracker).observe(&state);
        let _ = self.app.emit("hn-state", state);
        if alerts.is_empty() || !lock(&self.shared.inner).config.notifications {
            return;
        }
        crate::notifications::show_alerts(&self.app, &alerts);
    }

    async fn chat(&self, event: ChatEvent) {
        let _ = self.app.emit("hn-chat", event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pair_code_line_is_parsed() {
        let stdout = "pairing code: RANDTIA\nsingle use, valid for 600 seconds\n";
        assert_eq!(pair_code_from(stdout).unwrap(), "RANDTIA");
        assert!(pair_code_from("no devices paired").is_err());
    }
}
