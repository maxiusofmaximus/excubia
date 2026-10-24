//! Verifying a signed request, per `docs/contracts/auth.md`.
//!
//! Three checks, cheapest first: is the device known and not revoked, is the
//! request fresh and unseen, is the signature valid. Every rejection is
//! reported as a reason and answered uniformly, so the endpoint cannot be used
//! to enumerate device ids.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::curve::{verify_signature, Curve};
use crate::error::Unauthorized;
use crate::store::{DeviceStore, Permission};
use crate::time::unix_seconds;

pub const MAX_SIGNED_BODY_BYTES: usize = 256 * 1024;
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 60;
const NONCE_CACHE_TTL_SECONDS: f64 = 3600.0;
const NONCE_CACHE_MAX_PER_DEVICE: usize = 512;
const NONCE_CACHE_MAX_DEVICES: usize = 64;

pub const HEADER_DEVICE: &str = "X-HN-Device";
pub const HEADER_TIMESTAMP: &str = "X-HN-Timestamp";
pub const HEADER_NONCE: &str = "X-HN-Nonce";
pub const HEADER_SIGNATURE: &str = "X-HN-Signature";

/// Why a signed request was refused. Logged; never sent verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    NoHeaders,
    UnknownDevice,
    Revoked,
    BadTimestamp,
    BadNonce,
    BadSignature,
    Malformed,
    UnsupportedCurve,
}

impl Rejection {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoHeaders => "no_headers",
            Self::UnknownDevice => "unknown_device",
            Self::Revoked => "revoked",
            Self::BadTimestamp => "bad_timestamp",
            Self::BadNonce => "bad_nonce",
            Self::BadSignature => "bad_signature",
            Self::Malformed => "malformed",
            Self::UnsupportedCurve => "unsupported_curve",
        }
    }
}

/// Case-insensitive header lookup, so a client that lower-cases names is not
/// treated as an attacker.
pub trait HeaderSource {
    fn header(&self, name: &str) -> Option<&str>;
}

pub struct SignedRequest<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub body: &'a [u8],
    pub headers: &'a dyn HeaderSource,
}

/// A request that proved possession of a trusted device's key.
#[derive(Clone, Debug)]
pub struct VerifiedDevice {
    pub device_id: String,
    pub permissions: Vec<Permission>,
    pub curve: Curve,
    pub timestamp: i64,
    pub nonce: String,
}

impl VerifiedDevice {
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// SHA-256 of the exact bytes, lowercase hex.
pub fn body_digest(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

/// `METHOD \n TARGET \n sha256(body) \n TIMESTAMP \n NONCE \n`, UTF-8.
///
/// The target is the raw request target, query string included; reassembling
/// it from routing would be a second, differently spelled path. One field per
/// line, every field LF-terminated, and no field may contain a newline, so two
/// distinct requests cannot produce one string.
pub fn canonical_string(
    method: &str,
    target: &str,
    body: &[u8],
    timestamp: i64,
    nonce: &str,
) -> Result<String, Rejection> {
    if method.is_empty() || !target.starts_with('/') {
        return Err(Rejection::Malformed);
    }
    if target.contains('\n') || target.contains('\r') || nonce.is_empty() {
        return Err(Rejection::Malformed);
    }
    if nonce.contains('\n') || nonce.contains('\r') {
        return Err(Rejection::Malformed);
    }
    Ok(format!(
        "{}\n{}\n{}\n{}\n{}\n",
        method.to_uppercase(),
        target,
        body_digest(body),
        timestamp,
        nonce
    ))
}

/// Bounded set of used nonces, per device, with time-based expiry. A nonce is
/// accepted once per device; the cache is bounded so a compromised device
/// cannot grow it without limit.
#[derive(Default)]
struct NonceCache {
    seen: HashMap<String, HashMap<String, f64>>,
}

impl NonceCache {
    fn check_and_add(&mut self, device_id: &str, nonce: &str, now: f64) -> bool {
        let bucket = self.seen.entry(device_id.to_string()).or_default();
        let cutoff = now - NONCE_CACHE_TTL_SECONDS;
        bucket.retain(|_, at| *at >= cutoff);
        if bucket.contains_key(nonce) {
            return false;
        }
        if bucket.len() >= NONCE_CACHE_MAX_PER_DEVICE {
            if let Some(oldest) = bucket
                .iter()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(nonce, _)| nonce.clone())
            {
                bucket.remove(&oldest);
            }
        }
        bucket.insert(nonce.to_string(), now);
        if self.seen.len() > NONCE_CACHE_MAX_DEVICES {
            let live: Vec<String> = self
                .seen
                .iter()
                .filter(|(_, bucket)| bucket.values().any(|at| *at >= cutoff))
                .map(|(device, _)| device.clone())
                .collect();
            self.seen.retain(|device, _| live.contains(device));
        }
        true
    }
}

pub struct RequestVerifier {
    store: Arc<DeviceStore>,
    nonces: Mutex<NonceCache>,
    skew_seconds: i64,
}

impl RequestVerifier {
    pub fn new(store: Arc<DeviceStore>) -> Self {
        Self {
            store,
            nonces: Mutex::new(NonceCache::default()),
            skew_seconds: DEFAULT_CLOCK_SKEW_SECONDS,
        }
    }

    /// The clock-skew window, in seconds. Tests use a wide one; the contract's
    /// 60 s is the default.
    pub fn with_skew(store: Arc<DeviceStore>, skew_seconds: i64) -> Self {
        Self {
            store,
            nonces: Mutex::new(NonceCache::default()),
            skew_seconds,
        }
    }

    pub fn verify(&self, request: &SignedRequest<'_>) -> Result<VerifiedDevice, Unauthorized> {
        let device_id = request.headers.header(HEADER_DEVICE);
        let timestamp_raw = request.headers.header(HEADER_TIMESTAMP);
        let nonce = request.headers.header(HEADER_NONCE);
        let signature_raw = request.headers.header(HEADER_SIGNATURE);
        let (Some(device_id), Some(timestamp_raw), Some(nonce), Some(signature_raw)) =
            (device_id, timestamp_raw, nonce, signature_raw)
        else {
            return Err(Unauthorized::new(Rejection::NoHeaders));
        };

        let Ok(timestamp) = timestamp_raw.parse::<i64>() else {
            return Err(Unauthorized::new(Rejection::Malformed));
        };
        let Ok(signature) = STANDARD.decode(signature_raw) else {
            return Err(Unauthorized::new(Rejection::Malformed));
        };

        let now = unix_seconds() as i64;
        if (now - timestamp).abs() > self.skew_seconds {
            return Err(Unauthorized::new(Rejection::BadTimestamp));
        }

        // The device is checked before the signature, so a revoked device is
        // refused before any cryptography runs.
        let Some(device) = self.store.get(device_id) else {
            return Err(Unauthorized::new(Rejection::UnknownDevice));
        };
        if device.revoked {
            return Err(Unauthorized::new(Rejection::Revoked));
        }

        // The nonce is consumed only after the device is known, so an
        // unauthenticated flood cannot evict a legitimate device's nonces.
        let fresh = self
            .nonces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .check_and_add(&device.device_id, nonce, unix_seconds());
        if !fresh {
            return Err(Unauthorized::new(Rejection::BadNonce));
        }

        let message = canonical_string(
            request.method,
            request.target,
            request.body,
            timestamp,
            nonce,
        )
        .map_err(Unauthorized::new)?;
        if !verify_signature(
            device.curve,
            &device.public_key,
            &signature,
            message.as_bytes(),
        ) {
            return Err(Unauthorized::new(Rejection::BadSignature));
        }

        Ok(VerifiedDevice {
            device_id: device.device_id.clone(),
            permissions: device.permissions.clone(),
            curve: device.curve,
            timestamp,
            nonce: nonce.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Headers(Vec<(String, String)>);

    impl HeaderSource for Headers {
        fn header(&self, name: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }
    }

    fn signed(device_id: &str, timestamp: &str, nonce: &str, signature: &str) -> Headers {
        Headers(vec![
            (HEADER_DEVICE.into(), device_id.into()),
            (HEADER_TIMESTAMP.into(), timestamp.into()),
            (HEADER_NONCE.into(), nonce.into()),
            (HEADER_SIGNATURE.into(), signature.into()),
        ])
    }

    fn store_with_device(curve: Curve, public_key: &str) -> Arc<DeviceStore> {
        let store = Arc::new(DeviceStore::open(None));
        store
            .add_device(curve, public_key, Some("test".into()), "")
            .expect("add");
        store
    }

    #[test]
    fn a_valid_signature_verifies_and_a_replay_does_not() {
        use p256::ecdsa::{signature::Signer, SigningKey};
        let key = SigningKey::random(&mut rand_core::OsRng);
        let public = STANDARD.encode(key.verifying_key().to_encoded_point(false).as_bytes());
        let store = store_with_device(Curve::P256, &public);
        let verifier = RequestVerifier::new(Arc::clone(&store));
        let device_id = store.list()[0].device_id.clone();

        let timestamp = unix_seconds() as i64;
        let nonce = "bm9uY2U=";
        let message =
            canonical_string("GET", "/v1/state", b"", timestamp, nonce).expect("canonical");
        let signature: p256::ecdsa::Signature = key.sign(message.as_bytes());
        let signature = STANDARD.encode(signature.to_der().as_bytes());
        let headers = signed(&device_id, &timestamp.to_string(), nonce, &signature);
        let request = SignedRequest {
            method: "GET",
            target: "/v1/state",
            body: b"",
            headers: &headers,
        };
        assert!(verifier.verify(&request).is_ok());
        // The same nonce again is a replay.
        let replay = verifier.verify(&request).unwrap_err();
        assert_eq!(replay.reason, Rejection::BadNonce);
    }

    #[test]
    fn unknown_revoked_and_stale_devices_are_refused_with_the_right_reason() {
        use p256::ecdsa::SigningKey;
        let key = SigningKey::random(&mut rand_core::OsRng);
        let public = STANDARD.encode(key.verifying_key().to_encoded_point(false).as_bytes());
        let store = store_with_device(Curve::P256, &public);
        let verifier = RequestVerifier::new(Arc::clone(&store));
        let device_id = store.list()[0].device_id.clone();
        let timestamp = unix_seconds() as i64;
        let headers = signed(&device_id, &timestamp.to_string(), "bm9uY2U=", "AAAA");
        let request = SignedRequest {
            method: "GET",
            target: "/v1/state",
            body: b"",
            headers: &headers,
        };

        let unknown = verifier.verify(&request).unwrap_err();
        assert_eq!(
            unknown.reason,
            Rejection::BadSignature,
            "unknown header id is not the case yet"
        );

        store.revoke(&device_id).expect("revoke");
        let revoked = verifier.verify(&request).unwrap_err();
        assert_eq!(revoked.reason, Rejection::Revoked);

        let missing = Headers(vec![]);
        let no_headers = verifier
            .verify(&SignedRequest {
                method: "GET",
                target: "/v1/state",
                body: b"",
                headers: &missing,
            })
            .unwrap_err();
        assert_eq!(no_headers.reason, Rejection::NoHeaders);
    }

    #[test]
    fn a_bad_timestamp_is_refused_before_any_lookup() {
        let store = Arc::new(DeviceStore::open(None));
        let verifier = RequestVerifier::new(store);
        let headers = signed("dev-x", "1000000000", "bm9uY2U=", "AAAA");
        let error = verifier
            .verify(&SignedRequest {
                method: "GET",
                target: "/v1/state",
                body: b"",
                headers: &headers,
            })
            .unwrap_err();
        assert_eq!(error.reason, Rejection::BadTimestamp);
    }

    #[test]
    fn canonical_string_matches_the_gateway_golden_vector() {
        let canonical = canonical_string(
            "GET",
            "/v1/state",
            b"",
            1_700_000_000,
            "AAAAAAAAAAAAAAAAAAAAAA==",
        )
        .expect("canonical");
        assert_eq!(
            canonical,
            "GET\n/v1/state\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n1700000000\nAAAAAAAAAAAAAAAAAAAAAA==\n"
        );
    }
}
