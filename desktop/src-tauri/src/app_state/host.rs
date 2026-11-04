use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{lock, AppState};
use crate::app_folders::{local_dir, roaming_dir};
use crate::host_process::{self, HostConfig, HostEvent};
use crate::lan;

const LOCAL_HOST_LABEL: &str = "Excubia";

/// What the local core is doing, for the UI to render.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum HostStatus {
    Disabled,
    Starting,
    Ready { port: u16 },
    Failed { error: String },
}

impl AppState {
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
}

/// The app-owned data directory the local host (and its `agents.toml`) uses.
pub fn host_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    match super::profile_root() {
        Some(root) => Ok(root.join("core")),
        None => roaming_dir(app).map(|directory| directory.join("core")),
    }
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
    let log_dir = match super::profile_root() {
        Some(root) => root.join("logs"),
        None => local_dir(app)?.join("logs"),
    };
    let log_path = log_dir.join("host.log");
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
