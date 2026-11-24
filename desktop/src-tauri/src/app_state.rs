use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use excubia_desktop_core::identity::{DeviceIdentity, IdentityStore, KeyringIdentityStore};
use excubia_desktop_core::state::StateStore;
use excubia_desktop_core::stream::ConnectionStatus;
use excubia_desktop_core::{ChatFeed, CoreError, HostClient, Transport, WriteAction};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::app_folders::roaming_dir;
use crate::config::Config;
use crate::host_process::HostConfig;
use identity_migration::MigratingStore;

mod host;
mod identity_migration;
mod pairing;
mod stream;

pub use host::{host_data_dir, HostStatus};
pub use pairing::pair_code_from;

const KEYRING_SERVICE: &str = "dev.excubia.desktop";
/// legacy: the service before the rename: its identity is adopted, never written again.
const LEGACY_KEYRING_SERVICE: &str = "dev.herdrnotify.desktop";
const KEYRING_ACCOUNT: &str = "device-identity-v1";
const PROFILE_ENV: &str = "EXCUBIA_DESKTOP_PROFILE";

/// Test instances set `EXCUBIA_DESKTOP_PROFILE` to a folder: config, data, logs and the
/// keyring entry all move there, so the installed app's data is never read or written.
pub fn profile_root() -> Option<PathBuf> {
    std::env::var_os(PROFILE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn keyring_account() -> String {
    match profile_root() {
        Some(root) => format!("{KEYRING_ACCOUNT}-profile-{}", root.display()),
        None => KEYRING_ACCOUNT.to_string(),
    }
}

pub struct Inner {
    pub config: Config,
    pub identity: Option<DeviceIdentity>,
    /// Kept structured: the text is chosen at `bootstrap`, after the shell has sent its language.
    pub identity_error: Option<CoreError>,
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
    /// Set by the first `bootstrap`; see `claims_boot`.
    boot_claimed: AtomicBool,
}

/// True for the first bootstrap of the process in local mode, which starts the local host.
/// A window reload calls bootstrap again: it must not start a second sidecar.
pub fn claims_boot(claimed: &AtomicBool, local: bool) -> bool {
    let first = !claimed.swap(true, Ordering::SeqCst);
    first && local
}

/// A poisoned lock is not a reason to take the app down: the guarded data is
/// still usable, and every accessor here recovers instead of panicking.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything a signed request needs, read once per command.
pub struct HostSettings {
    pub url: String,
    pub identity: DeviceIdentity,
    pub transport: Transport,
    /// Read under the same lock as the identity and URL: the local server is this device.
    pub local: bool,
}

impl HostSettings {
    pub fn client(&self) -> Result<HostClient, CoreError> {
        HostClient::with_transport(&self.url, self.identity.clone(), &self.transport)
    }
}

impl AppState {
    pub fn new(app: &AppHandle) -> Self {
        let config_path = profile_root()
            .map(Ok)
            .unwrap_or_else(|| roaming_dir(app))
            .map(|directory| directory.join("config.json"))
            .unwrap_or_else(|_| PathBuf::from("desktop-config.json"));
        let mut config = Config::load(&config_path);
        let keyring = KeyringIdentityStore::new(KEYRING_SERVICE, keyring_account());
        let legacy = KeyringIdentityStore::new(LEGACY_KEYRING_SERVICE, keyring_account());
        let (identity, identity_error) = load_identity(&keyring, &legacy, &mut config);
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
            boot_claimed: AtomicBool::new(false),
        }
    }

    pub fn save_config(&self) -> Result<(), String> {
        let inner = lock(&self.inner);
        inner
            .config
            .save(&self.config_path)
            .map_err(|error| error.to_string())
    }

    pub fn settings(&self) -> Result<HostSettings, String> {
        let inner = lock(&self.inner);
        let identity = inner
            .identity
            .clone()
            .ok_or_else(|| {
                excubia_desktop_core::ui_lang::pick(
                    "No hay identidad de dispositivo disponible.",
                    "No device identity is available.",
                )
                .to_string()
            })?;
        let url = inner
            .runtime_url
            .clone()
            .unwrap_or_else(|| inner.config.host_url.clone());
        Ok(HostSettings {
            url,
            identity,
            transport: inner.config.transport(),
            local: inner.config.local,
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
            .map_err(|error| error.describe(WriteAction::Read).text())?;
        lock(&self.inner).identity = Some(identity);
        Ok(())
    }
}

fn load_identity(
    keyring: &KeyringIdentityStore,
    legacy: &KeyringIdentityStore,
    config: &mut Config,
) -> (Option<DeviceIdentity>, Option<CoreError>) {
    let store = MigratingStore {
        current: keyring,
        legacy,
    };
    match DeviceIdentity::load_or_create(
        &store,
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
            if let Err(error) = store.save(&identity) {
                eprintln!("no se pudo normalizar la identidad en el almacén seguro: {error}");
            }
            (Some(identity), None)
        }
        Err(error) => (None, Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reload condition: a second bootstrap (window reload) starts nothing.
    #[test]
    fn only_the_first_bootstrap_starts_the_local_host() {
        let claimed = AtomicBool::new(false);
        assert!(claims_boot(&claimed, true));
        assert!(!claims_boot(&claimed, true));
        assert!(!claims_boot(&claimed, true));
    }

    #[test]
    fn a_remote_first_bootstrap_starts_nothing() {
        let claimed = AtomicBool::new(false);
        assert!(!claims_boot(&claimed, false));
        assert!(!claims_boot(&claimed, true));
    }
}
