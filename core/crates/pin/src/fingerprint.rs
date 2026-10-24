use std::fmt;
use std::str::FromStr;

use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};

use crate::PinError;

/// SHA-256 of a certificate's SubjectPublicKeyInfo: the value a device pins.
/// Its text form is unpadded base64url, 43 characters, safe in a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub fn of_spki(spki_der: &[u8]) -> Self {
        Self(Sha256::digest(spki_der).into())
    }
}

impl FromStr for Fingerprint {
    type Err = PinError;

    fn from_str(text: &str) -> Result<Self, PinError> {
        let bytes = BASE64URL_NOPAD
            .decode(text.as_bytes())
            .map_err(|_| PinError::MalformedFingerprint)?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| PinError::MalformedFingerprint)?;
        Ok(Self(bytes))
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&BASE64URL_NOPAD.encode(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_form_round_trips() {
        let fingerprint = Fingerprint::of_spki(b"spki");
        let text = fingerprint.to_string();
        assert_eq!(text.len(), 43);
        assert_eq!(text.parse::<Fingerprint>().expect("parse"), fingerprint);
    }

    #[test]
    fn malformed_fingerprints_are_typed_errors() {
        for text in ["", "abc", "!".repeat(43).as_str(), &"A".repeat(44)] {
            assert!(
                matches!(
                    text.parse::<Fingerprint>(),
                    Err(PinError::MalformedFingerprint)
                ),
                "{text:?} must be refused"
            );
        }
    }
}
