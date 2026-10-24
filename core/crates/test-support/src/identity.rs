//! A P-256 identity for tests and the demo client.
//!
//! Adapted from `desktop/core/src/identity.rs` (MIT): same key encoding
//! (uncompressed SEC1 point, base64), same fingerprint (first 8 bytes of
//! SHA-256 over the point), same DER ECDSA signatures.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

/// The demo/test device identity. The private scalar lives only in memory, or
/// in the demo client's own file — the host never sees it.
#[derive(Clone)]
pub struct DemoIdentity {
    device_id: String,
    label: String,
    signing_key: SigningKey,
}

impl DemoIdentity {
    pub fn generate(device_id: impl Into<String>, label: impl Into<String>) -> Self {
        let device_id = device_id.into();
        let device_id = if device_id.trim().is_empty() {
            random_device_id()
        } else {
            device_id.trim().to_string()
        };
        Self {
            device_id,
            label: label.into(),
            signing_key: SigningKey::random(&mut OsRng),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The host assigns the device id at pairing; the client must adopt it.
    pub fn set_device_id(&mut self, device_id: impl Into<String>) {
        let device_id = device_id.into();
        if !device_id.trim().is_empty() {
            self.device_id = device_id.trim().to_string();
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Uncompressed SEC1 point, 65 bytes, base64: what the host verifies.
    pub fn public_key_b64(&self) -> String {
        let point = self.signing_key.verifying_key().to_encoded_point(false);
        STANDARD.encode(point.as_bytes())
    }

    /// First eight bytes of SHA-256 over the point, lowercase, colon-separated.
    pub fn fingerprint(&self) -> String {
        let point = self.signing_key.verifying_key().to_encoded_point(false);
        let digest = Sha256::digest(point.as_bytes());
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// DER ECDSA P-256 + SHA-256, base64 — `SHA256withECDSA`, the contract's
    /// encoding.
    pub fn sign(&self, canonical: &str) -> String {
        let signature: Signature = self.signing_key.sign(canonical.as_bytes());
        STANDARD.encode(signature.to_der().as_bytes())
    }

    /// A private scalar for the demo client's own key file.
    pub fn private_key_b64(&self) -> String {
        STANDARD.encode(self.signing_key.to_bytes())
    }

    pub fn from_private_key(device_id: &str, label: &str, private_key_b64: &str) -> Option<Self> {
        let raw = STANDARD.decode(private_key_b64).ok()?;
        let bytes: [u8; 32] = raw.as_slice().try_into().ok()?;
        Some(Self {
            device_id: device_id.to_string(),
            label: label.to_string(),
            signing_key: SigningKey::from_slice(&bytes).ok()?,
        })
    }
}

pub fn random_device_id() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    format!("dev-{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_key_is_an_uncompressed_sec1_point() {
        let identity = DemoIdentity::generate("dev-test", "test");
        let raw = STANDARD.decode(identity.public_key_b64()).expect("base64");
        assert_eq!(raw.len(), 65);
        assert_eq!(raw[0], 0x04);
    }

    #[test]
    fn the_fingerprint_is_stable_and_short() {
        let identity = DemoIdentity::generate("dev-test", "test");
        assert_eq!(identity.fingerprint().split(':').count(), 8);
        assert_eq!(identity.fingerprint(), identity.fingerprint());
    }

    #[test]
    fn a_private_key_round_trips() {
        let identity = DemoIdentity::generate("dev-test", "test");
        let restored = DemoIdentity::from_private_key(
            identity.device_id(),
            identity.label(),
            &identity.private_key_b64(),
        )
        .expect("restore");
        assert_eq!(restored.public_key_b64(), identity.public_key_b64());
    }
}
