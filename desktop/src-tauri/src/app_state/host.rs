use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use excubia_desktop_core::ui_lang::{self, Localized};
use serde::{Serialize, Serializer};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{claims_boot, lock, AppState};
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
    Failed {
        /// Kept in both languages; written in the language current at each send.
        #[serde(serialize_with = "shown_text")]
        error: Localized,
    },
}

fn shown_text<S: Serializer>(text: &Localized, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(text.text(ui_lang::current()))
}

impl AppState {
    /// Starts the local host from the first bootstrap only. The shell sends its language
    /// before bootstrap, so a startup failure is worded in that language. A reload starts nothing.
    pub fn boot_local_host(self: &Arc<Self>, app: AppHandle) {
        let local = lock(&self.inner).config.local;
        if claims_boot(&self.boot_claimed, local) {
            self.start_local_host(app);
        }
    }

    /// Spawns the bundled core and, once it is listening, pairs this device
    /// automatically and connects. The user never sees a code in local mode.
    pub fn start_local_host(self: &Arc<Self>, app: AppHandle) {
        self.stop_local_host();

        let pairing_binds = if lock(&self.inner).config.lan_pairing {
            lan::pairing_binds()
        } else {
            Vec::new()
        };
        let host_config = match host_config_for(&app, &pairing_binds) {
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

    fn fail_host(&self, app: &AppHandle, error: impl Into<Localized>) {
        let status = HostStatus::Failed {
            error: error.into(),
        };
        lock(&self.inner).host = status.clone();
        let _ = app.emit("hn-host", status);
    }
}

/// The sidecar is missing from the app folder. Both wordings are kept, so a later language change still reads right.
fn missing_binary(exe_dir: &std::path::Path) -> Localized {
    Localized::build(|lang| {
        format!(
            "{} {}; {}",
            lang.pick("no encuentro el núcleo local en", "the local core is not in"),
            exe_dir.display(),
            lang.pick("ejecuta scripts/build-host.ps1", "run scripts/build-host.ps1"),
        )
    })
}

/// The app-owned data directory the local host (and its `agents.toml`) uses.
pub fn host_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    match super::profile_root() {
        Some(root) => Ok(root.join("core")),
        None => roaming_dir(app).map(|directory| directory.join("core")),
    }
}

/// The sidecar plus the app-owned data and log directories.
fn host_config_for(app: &AppHandle, pairing_binds: &[SocketAddr]) -> Result<HostConfig, Localized> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
        .ok_or_else(|| {
            Localized::build(|lang| {
                lang.pick(
                    "no pude resolver el directorio de la aplicación",
                    "could not find the application directory",
                )
                .to_string()
            })
        })?;
    let binary = host_process::host_binary_path(&exe_dir)
        .ok_or_else(|| missing_binary(&exe_dir))?;
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
    for address in pairing_binds {
        args.push("--tls-bind".to_string());
        args.push(address.to_string());
    }
    if let Some(plugin) = host_process::uac_plugin_path(&exe_dir) {
        args.push("--uac-plugin-dir".to_string());
        args.push(plugin.display().to_string());
    }
    // legacy: HN_CORE_SPAWN was the spawn list before the rename; still read for existing setups.
    let spawn_list =
        std::env::var("EXCUBIA_CORE_SPAWN").or_else(|_| std::env::var("HN_CORE_SPAWN"));
    if let Ok(value) = spawn_list {
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

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_desktop_core::ui_lang::UiLang;
    use std::path::Path;

    #[test]
    fn a_missing_sidecar_stored_once_reads_in_the_language_asked_for() {
        // Stored as a Spanish-start failure, then read after a switch to English: no reload of the host, no global set.
        let status = HostStatus::Failed {
            error: missing_binary(Path::new("C:/app")),
        };
        let HostStatus::Failed { error } = status else {
            panic!("expected a failed host");
        };
        assert_eq!(
            error.text(UiLang::Es),
            "no encuentro el núcleo local en C:/app; ejecuta scripts/build-host.ps1"
        );
        assert_eq!(
            error.text(UiLang::En),
            "the local core is not in C:/app; run scripts/build-host.ps1"
        );
    }
}
