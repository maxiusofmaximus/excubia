use base64::{engine::general_purpose::STANDARD, Engine as _};
use p256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
use p256::elliptic_curve::Generate;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

const STORED_VERSION: u8 = 1;
const FINGERPRINT_BYTES: usize = 8;

/// The gateway caps `label` at 64 characters (`PairRequest`).
pub const MAX_DEVICE_LABEL_CHARS: usize = 64;

/// A P-256 identity. The private scalar is only ever serialised into the
/// platform secure store, never into the app config or a log line.
#[derive(Clone)]
pub struct DeviceIdentity {
    device_id: String,
    label: String,
    signing_key: SigningKey,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    version: u8,
    device_id: String,
    label: String,
    private_key: String,
}

impl DeviceIdentity {
    pub fn generate(device_id: impl Into<String>, label: impl Into<String>) -> Self {
        let device_id = device_id.into();
        let device_id = if device_id.trim().is_empty() {
            random_device_id()
        } else {
            device_id.trim().to_string()
        };
        let signing_key = SigningKey::generate();
        Self {
            device_id,
            label: label.into(),
            signing_key,
        }
    }

    /// Loads the identity or creates one on first launch.
    pub fn load_or_create(
        store: &dyn IdentityStore,
        default_id: &str,
        default_label: &str,
    ) -> Result<Self> {
        if let Some(mut identity) = store.load()? {
            if identity.device_id.trim().is_empty() {
                identity.device_id = random_device_id();
            }
            if identity.label.trim().is_empty() {
                identity.label = default_label.to_string();
            }
            return Ok(identity);
        }
        let identity = Self::generate(default_id, default_label);
        store.save(&identity)?;
        Ok(identity)
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn set_device_id(&mut self, device_id: impl Into<String>) {
        let device_id = device_id.into();
        if !device_id.trim().is_empty() {
            self.device_id = device_id.trim().to_string();
        }
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Public key material, for callers that verify their own signatures
    /// (tests) or publish the identity.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key().clone()
    }

    /// Uncompressed SEC1 point, 65 bytes, base64: what the gateway verifies.
    pub fn public_key_b64(&self) -> String {
        let point = self.verifying_key().to_sec1_point(false);
        STANDARD.encode(point.as_bytes())
    }

    /// First eight bytes of SHA-256 over the SEC1 point, lowercase and colon
    /// separated, matching the Android pairing screen.
    pub fn fingerprint(&self) -> String {
        let point = self.verifying_key().to_sec1_point(false);
        let digest = Sha256::digest(point.as_bytes());
        digest[..FINGERPRINT_BYTES]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    pub(crate) fn sign_canonical(&self, canonical: &[u8]) -> String {
        let signature: Signature = self.signing_key.sign(canonical);
        STANDARD.encode(signature.to_der().as_bytes())
    }

    fn stored_json(&self) -> Result<String> {
        let stored = StoredIdentity {
            version: STORED_VERSION,
            device_id: self.device_id.clone(),
            label: self.label.clone(),
            private_key: STANDARD.encode(self.signing_key.to_bytes()),
        };
        serde_json::to_string(&stored).map_err(CoreError::from)
    }

    fn from_stored_json(json: &str) -> Result<Self> {
        let stored: StoredIdentity = serde_json::from_str(json)
            .map_err(|error| CoreError::SecureStore(format!("identidad ilegible: {error}")))?;
        if stored.version != STORED_VERSION {
            return Err(CoreError::SecureStore(format!(
                "versión de identidad no soportada: {}",
                stored.version
            )));
        }
        let raw = STANDARD
            .decode(stored.private_key)
            .map_err(|error| CoreError::SecureStore(format!("clave privada ilegible: {error}")))?;
        if raw.len() != 32 {
            return Err(CoreError::SecureStore(
                "la clave privada no mide 32 bytes".to_string(),
            ));
        }
        let signing_key = SigningKey::from_slice(&raw)
            .map_err(|error| CoreError::SecureStore(format!("clave privada inválida: {error}")))?;
        Ok(Self {
            device_id: stored.device_id,
            label: stored.label,
            signing_key,
        })
    }
}

pub fn random_device_id() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    format!("dev-{}", hex::encode(bytes))
}

/// A sensible name for this machine: "Escritorio `<host>`", or "Escritorio" when
/// the host name is not available.
pub fn default_device_label() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty());
    let label = match host {
        Some(host) => format!("Escritorio {host}"),
        None => "Escritorio".to_string(),
    };
    label.chars().take(MAX_DEVICE_LABEL_CHARS).collect()
}

/// Trims and checks a label before it is stored or sent to the gateway.
pub fn validate_device_label(value: &str) -> Result<String> {
    let label = value.trim();
    if label.is_empty() {
        return Err(CoreError::InvalidInput(
            "el nombre del equipo no puede estar vacío".to_string(),
        ));
    }
    if label.chars().count() > MAX_DEVICE_LABEL_CHARS {
        return Err(CoreError::InvalidInput(format!(
            "el nombre del equipo no puede pasar de {MAX_DEVICE_LABEL_CHARS} caracteres"
        )));
    }
    if label.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput(
            "el nombre del equipo no admite saltos de línea ni caracteres de control".to_string(),
        ));
    }
    Ok(label.to_string())
}

/// For values read from disk: a corrupt label falls back to the default rather
/// than leaving the app without a name to pair with.
pub fn normalize_device_label(value: &str) -> String {
    validate_device_label(value).unwrap_or_else(|_| default_device_label())
}

/// Where the private scalar lives. Desktop uses the OS secure store; Android
/// supplies a Keystore-backed implementation without changing this crate.
pub trait IdentityStore: Send + Sync {
    fn load(&self) -> Result<Option<DeviceIdentity>>;
    fn save(&self, identity: &DeviceIdentity) -> Result<()>;
}

#[cfg(feature = "os-keyring")]
pub struct KeyringIdentityStore {
    service: String,
    account: String,
}

#[cfg(feature = "os-keyring")]
impl KeyringIdentityStore {
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            account: account.into(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| CoreError::SecureStore(error.to_string()))
    }
}

#[cfg(feature = "os-keyring")]
impl IdentityStore for KeyringIdentityStore {
    fn load(&self) -> Result<Option<DeviceIdentity>> {
        match self.entry()?.get_password() {
            Ok(json) => DeviceIdentity::from_stored_json(&json).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CoreError::SecureStore(error.to_string())),
        }
    }

    fn save(&self, identity: &DeviceIdentity) -> Result<()> {
        self.entry()?
            .set_password(&identity.stored_json()?)
            .map_err(|error| CoreError::SecureStore(error.to_string()))
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MemoryIdentityStore {
    pub value: std::sync::Mutex<Option<String>>,
}

#[cfg(test)]
impl IdentityStore for MemoryIdentityStore {
    fn load(&self) -> Result<Option<DeviceIdentity>> {
        match self.value.lock().unwrap().as_deref() {
            Some(json) => DeviceIdentity::from_stored_json(json).map(Some),
            None => Ok(None),
        }
    }

    fn save(&self, identity: &DeviceIdentity) -> Result<()> {
        *self.value.lock().unwrap() = Some(identity.stored_json()?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;

    #[test]
    fn public_key_is_an_uncompressed_sec1_point() {
        let identity = DeviceIdentity::generate("dev-test", "test");
        let raw = STANDARD.decode(identity.public_key_b64()).unwrap();
        assert_eq!(raw.len(), 65);
        assert_eq!(raw[0], 0x04);
    }

    #[test]
    fn signatures_are_der_and_verify() {
        let identity = DeviceIdentity::generate("dev-test", "test");
        let signature_b64 = identity.sign_canonical(b"hola\n");
        let der = STANDARD.decode(signature_b64).unwrap();
        let signature = Signature::from_der(&der).unwrap();
        identity
            .verifying_key()
            .verify(b"hola\n", &signature)
            .unwrap();
    }

    #[test]
    fn identity_survives_a_store_round_trip_and_keeps_signing() {
        let store = MemoryIdentityStore::default();
        let first = DeviceIdentity::load_or_create(&store, "dev-fallback", "PC").unwrap();
        let second = DeviceIdentity::load_or_create(&store, "dev-fallback", "PC").unwrap();
        assert_eq!(first.public_key_b64(), second.public_key_b64());
        assert_eq!(first.fingerprint(), second.fingerprint());
        let signature = STANDARD.decode(second.sign_canonical(b"x")).unwrap();
        let signature = Signature::from_der(&signature).unwrap();
        first.verifying_key().verify(b"x", &signature).unwrap();
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        let identity = DeviceIdentity::generate("dev-test", "test");
        let fingerprint = identity.fingerprint();
        assert_eq!(fingerprint.split(':').count(), FINGERPRINT_BYTES);
        assert_eq!(fingerprint, identity.fingerprint());
    }

    #[test]
    fn default_label_is_always_a_valid_label() {
        let label = default_device_label();
        assert!(!label.trim().is_empty());
        assert!(label.chars().count() <= MAX_DEVICE_LABEL_CHARS);
        assert_eq!(validate_device_label(&label).unwrap(), label);
    }

    #[test]
    fn label_validation_trims_and_accepts_a_normal_name() {
        assert_eq!(validate_device_label("  PC de Max  ").unwrap(), "PC de Max");
        assert_eq!(
            validate_device_label("a".repeat(64).as_str())
                .unwrap()
                .len(),
            64
        );
    }

    #[test]
    fn label_validation_rejects_empty_too_long_and_control_characters() {
        assert!(validate_device_label("").is_err());
        assert!(validate_device_label("   ").is_err());
        assert!(validate_device_label(&"a".repeat(MAX_DEVICE_LABEL_CHARS + 1)).is_err());
        assert!(validate_device_label("PC\nde Max").is_err());
        assert!(validate_device_label("PC\tde Max").is_err());
    }

    #[test]
    fn a_corrupt_label_falls_back_to_the_default() {
        assert_eq!(normalize_device_label("  "), default_device_label());
        assert_eq!(normalize_device_label("Taller"), "Taller");
    }
}
