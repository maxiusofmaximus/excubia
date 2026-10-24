//! Public-key verification, per `docs/contracts/auth.md`.
//!
//! The curve always comes from the device record, never from the request: a
//! caller must not be able to choose the algorithm that verifies its own
//! forged signature. A signature that is not valid DER simply fails.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey as Ed25519Key};
use p256::ecdsa::{signature::Verifier, Signature as P256Signature, VerifyingKey as P256Key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    P256,
    Ed25519,
}

impl Curve {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "p256" => Some(Self::P256),
            "ed25519" => Some(Self::Ed25519),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::P256 => "p256",
            Self::Ed25519 => "ed25519",
        }
    }
}

/// True when `public_key` is base64 of a point `curve` accepts: the test
/// verification applies, so a key that passes it can be used to verify.
pub fn is_valid_public_key(curve: Curve, public_key: &str) -> bool {
    let Ok(raw) = STANDARD.decode(public_key) else {
        return false;
    };
    match curve {
        Curve::P256 => p256_from_bytes(&raw).is_some(),
        Curve::Ed25519 => ed25519_from_bytes(&raw).is_some(),
    }
}

/// True when `signature` is a valid signature of `message` for `public_key`.
///
/// `public_key` is base64: an uncompressed SEC1 point (65 bytes) for P-256, or
/// 32 raw bytes for Ed25519. A key that does not decode, or a signature that is
/// not valid DER, is simply an invalid signature.
pub fn verify_signature(curve: Curve, public_key: &str, signature: &[u8], message: &[u8]) -> bool {
    let Ok(raw) = STANDARD.decode(public_key) else {
        return false;
    };
    match curve {
        Curve::P256 => {
            let Some(key) = p256_from_bytes(&raw) else {
                return false;
            };
            let Ok(signature) = P256Signature::from_der(signature) else {
                return false;
            };
            key.verify(message, &signature).is_ok()
        }
        Curve::Ed25519 => {
            let Some(key) = ed25519_from_bytes(&raw) else {
                return false;
            };
            let Ok(signature) = Ed25519Signature::from_slice(signature) else {
                return false;
            };
            key.verify_strict(message, &signature).is_ok()
        }
    }
}

fn p256_from_bytes(raw: &[u8]) -> Option<P256Key> {
    P256Key::from_sec1_bytes(raw).ok()
}

fn ed25519_from_bytes(raw: &[u8]) -> Option<Ed25519Key> {
    let key_bytes = <[u8; 32]>::try_from(raw).ok()?;
    Ed25519Key::from_bytes(&key_bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey as Ed25519SigningKey};
    use p256::ecdsa::{signature::Signer, SigningKey};
    use p256::elliptic_curve::Generate;

    fn p256_key() -> (SigningKey, String) {
        let key = SigningKey::generate();
        let public = STANDARD.encode(key.verifying_key().to_sec1_point(false).as_bytes());
        (key, public)
    }

    #[test]
    fn p256_der_signatures_verify() {
        let (key, public) = p256_key();
        let signature: P256Signature = key.sign(b"hola\n");
        assert!(verify_signature(
            Curve::P256,
            &public,
            signature.to_der().as_bytes(),
            b"hola\n"
        ));
        assert!(!verify_signature(
            Curve::P256,
            &public,
            b"not-der",
            b"hola\n"
        ));
        assert!(!verify_signature(
            Curve::P256,
            &public,
            signature.to_der().as_bytes(),
            b"otro"
        ));
    }

    #[test]
    fn a_raw_rs_signature_is_not_accepted() {
        let (key, public) = p256_key();
        let signature: P256Signature = key.sign(b"hola\n");
        let raw = signature.to_bytes();
        assert!(!verify_signature(Curve::P256, &public, &raw, b"hola\n"));
    }

    #[test]
    fn ed25519_raw_signatures_verify() {
        let key = Ed25519SigningKey::from_bytes(&[7u8; 32]);
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        let signature = key.sign(b"hola\n");
        assert!(verify_signature(
            Curve::Ed25519,
            &public,
            &signature.to_bytes(),
            b"hola\n"
        ));
        assert!(!verify_signature(
            Curve::Ed25519,
            &public,
            &signature.to_bytes(),
            b"otro"
        ));
    }

    #[test]
    fn public_keys_are_checked_for_their_curve() {
        let (_, public) = p256_key();
        assert!(is_valid_public_key(Curve::P256, &public));
        assert!(!is_valid_public_key(Curve::Ed25519, &public));
        assert!(!is_valid_public_key(
            Curve::P256,
            &STANDARD.encode([0u8; 10])
        ));
        assert!(!is_valid_public_key(Curve::P256, "no-base64!"));
    }

    #[test]
    fn malformed_keys_are_invalid_not_panics() {
        assert!(!verify_signature(Curve::P256, "no-base64!", b"x", b"x"));
        assert!(!verify_signature(
            Curve::P256,
            &STANDARD.encode([0u8; 10]),
            b"x",
            b"x"
        ));
        assert!(!verify_signature(
            Curve::Ed25519,
            &STANDARD.encode([0u8; 10]),
            b"x",
            b"x"
        ));
    }
}
