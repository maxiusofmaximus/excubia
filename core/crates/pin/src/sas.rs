//! The short authentication string of a pairing (RFC pairing-security §5, F1): the
//! phone and the host each compute it, and the Master checks the two match before
//! granting anything. It is not a secret: both inputs are public.

use sha2::{Digest, Sha256};

use crate::Fingerprint;

const DOMAIN: &[u8] = b"excubia-sas-v1\0";

/// SHA-256 of `DOMAIN ‖ host_pin ‖ device_public_key`, its first four bytes as a
/// big-endian `u32`, modulo 1 000 000, as six digits. `device_public_key` is the
/// SEC1 point the device sent to `POST /v1/pair`.
pub fn sas(host_pin: &Fingerprint, device_public_key: &[u8]) -> String {
    let digest = Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(host_pin.as_bytes())
        .chain_update(device_public_key)
        .finalize();
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", value % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors computed with Python's `hashlib` (independent of this code). The
    /// Dart app reproduces them: see `docs/contracts/pairing.md`.
    const PIN: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    const KEY_A: &str = "0411111111111111111111111111111111111111111111111111111111111111112222222222222222222222222222222222222222222222222222222222222222";
    const KEY_B: &str = "0433333333333333333333333333333333333333333333333333333333333333334444444444444444444444444444444444444444444444444444444444444444";

    fn bytes(hex_text: &str) -> Vec<u8> {
        data_encoding::HEXLOWER_PERMISSIVE
            .decode(hex_text.as_bytes())
            .expect("hex")
    }

    #[test]
    fn matches_the_fixed_vectors() {
        let pin: Fingerprint = PIN.parse().expect("pin");
        assert_eq!(sas(&pin, &bytes(KEY_A)), "097665");
        assert_eq!(sas(&pin, &bytes(KEY_B)), "827242");
    }

    #[test]
    fn a_different_key_or_pin_gives_a_different_string() {
        let pin: Fingerprint = PIN.parse().expect("pin");
        let other_pin = Fingerprint::of_spki(b"another host");
        assert_ne!(sas(&pin, &bytes(KEY_A)), sas(&pin, &bytes(KEY_B)));
        assert_ne!(sas(&pin, &bytes(KEY_A)), sas(&other_pin, &bytes(KEY_A)));
    }

    #[test]
    fn it_is_always_six_digits() {
        let pin: Fingerprint = PIN.parse().expect("pin");
        let text = sas(&pin, &bytes(KEY_A));
        assert_eq!(text.len(), 6);
        assert!(text.bytes().all(|byte| byte.is_ascii_digit()));
    }
}
