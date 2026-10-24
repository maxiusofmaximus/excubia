//! The exact bytes every request signs, per `docs/contracts/auth.md`.
//!
//! Canonicalisation is the part that fails in practice, so it lives on its own
//! and is pinned by the gateway's golden vector.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// `METHOD \n TARGET \n sha256(body) \n TIMESTAMP \n NONCE \n`, UTF-8.
pub fn canonical_string(
    method: &str,
    target: &str,
    body: &[u8],
    timestamp: u64,
    nonce: &str,
) -> String {
    format!(
        "{method}\n{target}\n{}\n{timestamp}\n{nonce}\n",
        body_digest(body)
    )
}

/// SHA-256 of the exact received bytes, lowercase hex.
pub fn body_digest(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

pub(crate) fn unix_seconds() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| CoreError::Protocol(format!("reloj del sistema: {error}")))
}

/// Base64 of 16 random bytes, unique per request.
pub(crate) fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_body_digest_matches_the_contract() {
        assert_eq!(
            body_digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn canonical_string_matches_the_gateway_golden_vector() {
        let canonical = canonical_string(
            "GET",
            "/v1/state",
            b"",
            1_700_000_000,
            "AAAAAAAAAAAAAAAAAAAAAA==",
        );
        assert_eq!(
            canonical,
            "GET\n/v1/state\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n1700000000\nAAAAAAAAAAAAAAAAAAAAAA==\n"
        );
    }
}
