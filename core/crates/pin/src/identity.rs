use std::time::{SystemTime, UNIX_EPOCH};

use rcgen::{CertificateParams, DnType, KeyPair};
use time::OffsetDateTime;

use crate::{CertificateInfo, Fingerprint, PinError};

/// A certificate that starts a day early still verifies on a home LAN whose
/// clock runs behind.
const BACKDATE_SECONDS: i64 = 24 * 60 * 60;
/// A pinned certificate that expires forces every device to pair again, so
/// the default lasts ten years; rotation is the deliberate way to change it.
pub const DEFAULT_VALIDITY_SECONDS: i64 = 10 * 365 * 24 * 60 * 60;

/// The host's TLS identity: a self-signed ECDSA P-256 certificate and its
/// PKCS#8 key. The key is secret; this type has no `Debug` so it cannot leak
/// into a log line.
#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

impl Identity {
    /// A new identity valid from a day ago for ten years.
    pub fn generate() -> Result<Self, PinError> {
        let now = unix_now();
        Self::generate_within(now - BACKDATE_SECONDS, now + DEFAULT_VALIDITY_SECONDS)
    }

    pub fn generate_within(not_before: i64, not_after: i64) -> Result<Self, PinError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec!["herdr-notify".to_string()])?;
        params
            .distinguished_name
            .push(DnType::CommonName, "herdr-notify host");
        params.not_before = datetime(not_before)?;
        params.not_after = datetime(not_after)?;
        let certificate = params.self_signed(&key)?;
        Ok(Self {
            cert_der: certificate.der().as_ref().to_vec(),
            key_der: key.serialize_der(),
        })
    }

    pub fn from_parts(cert_der: Vec<u8>, key_der: Vec<u8>) -> Self {
        Self { cert_der, key_der }
    }

    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    pub fn key_der(&self) -> &[u8] {
        &self.key_der
    }

    pub fn fingerprint(&self) -> Result<Fingerprint, PinError> {
        Ok(CertificateInfo::read(&self.cert_der)?.fingerprint())
    }
}

fn datetime(unix_seconds: i64) -> Result<OffsetDateTime, PinError> {
    OffsetDateTime::from_unix_timestamp(unix_seconds).map_err(|_| PinError::InvalidWindow)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_identity_pins_to_its_own_spki() {
        let identity = Identity::generate().expect("generate");
        let info = CertificateInfo::read(identity.cert_der()).expect("read");
        assert_eq!(identity.fingerprint().expect("pin"), info.fingerprint());
        assert!(info.check_at(unix_now()).is_ok());
    }

    #[test]
    fn two_identities_never_share_a_pin() {
        let first = Identity::generate().expect("generate");
        let second = Identity::generate().expect("generate");
        assert_ne!(first.fingerprint().unwrap(), second.fingerprint().unwrap());
    }
}
