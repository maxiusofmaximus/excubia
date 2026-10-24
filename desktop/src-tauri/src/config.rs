use std::path::Path;

use herdr_desktop_core::{
    default_device_label, normalize_device_label, Fingerprint, PinSet, Transport,
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_GATEWAY: &str = "http://127.0.0.1:8787";

/// The host key a pairing link pinned. Public, so it lives beside the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPins {
    pub primary: String,
    #[serde(default)]
    pub backup: Option<String>,
}

impl From<PinSet> for StoredPins {
    fn from(pins: PinSet) -> Self {
        Self {
            primary: pins.primary().to_string(),
            backup: pins.backup().map(|backup| backup.to_string()),
        }
    }
}

impl StoredPins {
    /// A corrupt entry reads as "no pin": the plain transport then trusts only
    /// the web PKI, so a self-signed host is refused rather than trusted.
    fn to_pins(&self) -> Option<PinSet> {
        let primary: Fingerprint = self.primary.parse().ok()?;
        let backup = self
            .backup
            .as_deref()
            .map(str::parse::<Fingerprint>)
            .transpose()
            .ok()?;
        Some(PinSet::new(primary, backup))
    }
}

/// Local, non-secret state. The private key lives in the OS credential store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The app hosts its own core by default; `false` means "another host".
    pub local: bool,
    pub gateway_url: String,
    pub gateway_pins: Option<StoredPins>,
    pub device_id: Option<String>,
    pub label: String,
    pub paired: bool,
    pub notifications: bool,
    /// The local core also listens on the LAN, so a phone can pair with its QR.
    pub lan_pairing: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            local: true,
            gateway_url: DEFAULT_GATEWAY.to_string(),
            gateway_pins: None,
            device_id: None,
            label: default_device_label(),
            paired: false,
            notifications: true,
            lan_pairing: false,
        }
    }
}

impl Config {
    /// A corrupt or partial file degrades to the defaults instead of leaving
    /// the app without a URL or a name to pair with.
    pub fn load(path: &Path) -> Self {
        let mut config = match std::fs::read_to_string(path) {
            Ok(body) => serde_json::from_str(&body).unwrap_or_else(|error| {
                eprintln!("configuración ilegible, se usan valores por defecto: {error}");
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        config.label = normalize_device_label(&config.label);
        if config.gateway_url.trim().is_empty() {
            config.gateway_url = DEFAULT_GATEWAY.to_string();
        }
        config
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, body)
    }

    /// How the gateway is reached: the local core is always plain loopback,
    /// a remote one is pinned when a pairing link pinned it.
    pub fn transport(&self) -> Transport {
        if self.local {
            return Transport::Plain;
        }
        self.gateway_pins
            .as_ref()
            .and_then(StoredPins::to_pins)
            .map_or(Transport::Plain, Transport::Pinned)
    }
}
