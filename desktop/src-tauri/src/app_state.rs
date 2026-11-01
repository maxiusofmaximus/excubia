use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use herdr_desktop_core::identity::{DeviceIdentity, IdentityStore, KeyringIdentityStore};
use herdr_desktop_core::state::StateStore;
use herdr_desktop_core::stream::ConnectionStatus;
use herdr_desktop_core::{ChatFeed, CoreError, GatewayClient, Transport};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::host_process::HostConfig;

mod host;
mod pairing;
mod stream;

pub use host::{host_data_dir, HostStatus};
pub use pairing::pair_code_from;

const KEYRING_SERVICE: &str = "dev.herdrnotify.desktop";
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
        let config_path = profile_root()
            .map(Ok)
            .unwrap_or_else(|| app.path().app_config_dir())
            .map(|directory| directory.join("config.json"))
            .unwrap_or_else(|_| PathBuf::from("desktop-config.json"));
        let mut config = Config::load(&config_path);
        let keyring = KeyringIdentityStore::new(KEYRING_SERVICE, keyring_account());
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
