//! Request signing for the gateway contract (`docs/contracts/auth.md`).
//!
//! The private key stays in `flutter_secure_storage`; Dart passes it for the
//! duration of a call. Nothing here persists key material.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedHeaders {
    pub timestamp: u32,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKeyMaterial {
    pub private_key_b64: String,
    pub public_key_b64: String,
    pub fingerprint: String,
}

/// Create the phone's P-256 key. The private scalar is returned so Dart can put
/// it in the platform secure store; it is never written by this crate.
#[flutter_rust_bridge::frb(sync)]
pub fn generate_device_key() -> DeviceKeyMaterial {
    let key = SigningKey::random(&mut OsRng);
    let private_key_b64 = STANDARD.encode(key.to_bytes());
    let public_key_b64 = encode_public(&key);
    let fingerprint = fingerprint_of(&key);
    DeviceKeyMaterial {
        private_key_b64,
        public_key_b64,
        fingerprint,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn public_key_from_private(private_key_b64: String) -> Option<String> {
    decode_key(&private_key_b64).map(|key| encode_public(&key))
}

#[flutter_rust_bridge::frb(sync)]
pub fn fingerprint_from_private(private_key_b64: String) -> Option<String> {
    decode_key(&private_key_b64).map(|key| fingerprint_of(&key))
}

/// Sign a request: `METHOD \n TARGET \n sha256(body) \n TIMESTAMP \n NONCE \n`.
///
/// Returns None only when the stored key is not a valid P-256 scalar; the
/// timestamp and nonce are generated here so the unit stays out of callers.
#[flutter_rust_bridge::frb(sync)]
pub fn sign_request(
    private_key_b64: String,
    method: String,
    target: String,
    body: Vec<u8>,
) -> Option<SignedHeaders> {
    let key = decode_key(&private_key_b64)?;
    let timestamp = unix_seconds()?;
    let nonce = random_nonce();
    let payload = canonical_string(&method, &target, &body, timestamp, &nonce);
    let signature: Signature = key.sign(payload.as_bytes());
    Some(SignedHeaders {
        timestamp,
        nonce,
        signature: STANDARD.encode(signature.to_der().as_bytes()),
    })
}

pub(crate) fn canonical_string(
    method: &str,
    target: &str,
    body: &[u8],
    timestamp: u32,
    nonce: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n",
        method.to_uppercase(),
        target,
        body_digest(body),
        timestamp,
        nonce
    )
}

pub(crate) fn body_digest(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

fn decode_key(private_key_b64: &str) -> Option<SigningKey> {
    let bytes = STANDARD.decode(private_key_b64).ok()?;
    SigningKey::from_slice(&bytes).ok()
}

fn encode_public(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_encoded_point(false).as_bytes())
}

fn fingerprint_of(key: &SigningKey) -> String {
    let point = key.verifying_key().to_encoded_point(false);
    let digest = Sha256::digest(point.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn unix_seconds() -> Option<u32> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u32::try_from(duration.as_secs()).ok())
}

fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};

    use super::*;

    const VECTORS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../gateway/tests/vectors/signing_vectors.json"
    ));

    #[test]
    fn the_empty_body_digest_matches_the_contract() {
        assert_eq!(
            body_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_shared_golden_vectors_still_match() {
        let file: serde_json::Value = serde_json::from_str(VECTORS).unwrap();
        let vectors = file["vectors"].as_array().unwrap();
        assert_eq!(vectors.len(), 6);

        let names: Vec<&str> = vectors
            .iter()
            .map(|vector| vector["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"read-state"));
        assert!(names.contains(&"respond-unicode"));

        for vector in vectors {
            let actual = canonical_string(
                vector["method"].as_str().unwrap(),
                vector["target"].as_str().unwrap(),
                vector["body"].as_str().unwrap().as_bytes(),
                vector["timestamp"].as_u64().unwrap() as u32,
                vector["nonce"].as_str().unwrap(),
            );
            assert_eq!(
                actual,
                vector["expected"].as_str().unwrap(),
                "vector {}",
                vector["name"]
            );
        }
    }

    #[test]
    fn a_generated_key_has_the_wire_shape() {
        let material = generate_device_key();
        let public = STANDARD.decode(&material.public_key_b64).unwrap();
        let private = STANDARD.decode(&material.private_key_b64).unwrap();
        assert_eq!(public.len(), 65);
        assert_eq!(public[0], 0x04);
        assert_eq!(private.len(), 32);

        let parts: Vec<&str> = material.fingerprint.split(':').collect();
        assert_eq!(parts.len(), 8);
        assert!(parts.iter().all(|part| part.len() == 2));
    }

    #[test]
    fn the_stored_private_key_derives_the_same_public_key_and_fingerprint() {
        let material = generate_device_key();
        assert_eq!(
            public_key_from_private(material.private_key_b64.clone()).unwrap(),
            material.public_key_b64
        );
        assert_eq!(
            fingerprint_from_private(material.private_key_b64.clone()).unwrap(),
            material.fingerprint
        );
    }

    #[test]
    fn a_signature_is_der_and_verifies_over_the_canonical_bytes() {
        let material = generate_device_key();
        let body = r#"{"text":"sí, hazlo"}"#.as_bytes().to_vec();
        let signed = sign_request(
            material.private_key_b64.clone(),
            "post".to_string(),
            "/v1/agents/w1:p1/respond".to_string(),
            body.clone(),
        )
        .unwrap();

        let payload = canonical_string(
            "POST",
            "/v1/agents/w1:p1/respond",
            &body,
            signed.timestamp,
            &signed.nonce,
        );
        let signature = Signature::from_der(&STANDARD.decode(&signed.signature).unwrap()).unwrap();
        let public = STANDARD.decode(&material.public_key_b64).unwrap();
        let verifying_key = VerifyingKey::from_sec1_bytes(&public).unwrap();

        assert!(verifying_key.verify(payload.as_bytes(), &signature).is_ok());
        assert!(verifying_key.verify(b"tampered", &signature).is_err());
    }

    #[test]
    fn nonces_and_timestamps_are_fresh_for_each_request() {
        let material = generate_device_key();
        let first = sign_request(
            material.private_key_b64.clone(),
            "GET".to_string(),
            "/v1/state".to_string(),
            Vec::new(),
        )
        .unwrap();
        let second = sign_request(
            material.private_key_b64,
            "GET".to_string(),
            "/v1/state".to_string(),
            Vec::new(),
        )
        .unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert_eq!(STANDARD.decode(&first.nonce).unwrap().len(), 16);
        assert!(first.timestamp > 0);
    }

    #[test]
    fn an_invalid_stored_key_signs_nothing() {
        assert!(sign_request(
            "not base64 at all!!".to_string(),
            "GET".to_string(),
            "/v1/state".to_string(),
            Vec::new(),
        )
        .is_none());
        assert!(sign_request(
            STANDARD.encode([0u8; 32]),
            "GET".to_string(),
            "/v1/state".to_string(),
            Vec::new(),
        )
        .is_none());
        assert!(public_key_from_private("nope".to_string()).is_none());
    }
}
