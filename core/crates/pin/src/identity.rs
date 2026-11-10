use std::time::{SystemTime, UNIX_EPOCH};

use rcgen::{CertificateParams, DnType, KeyPair};
use time::OffsetDateTime;
use x509_parser::certificate::X509Certificate;
use x509_parser::prelude::FromDer;

use crate::{CertificateInfo, Fingerprint, PinError};

/// A certificate that starts a day early still verifies on a home LAN whose
/// clock runs behind.
const BACKDATE_SECONDS: i64 = 24 * 60 * 60;
/// A pinned certificate that expires forces every device to pair again, so
/// the default lasts ten years; rotation is the deliberate way to change it.
pub const DEFAULT_VALIDITY_SECONDS: i64 = 10 * 365 * 24 * 60 * 60;
/// The name a host certificate carries now: the SNI the LAN transport asks for.
pub const CURRENT_NAME: &str = "excubia";
/// The common name a host certificate carries now.
const CURRENT_COMMON_NAME: &str = "excubia host";

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
        Self::generate_named(CURRENT_NAME, not_before, not_after)
    }

    /// An identity whose certificate names `name`. Only tests mint one under an
    /// old name, to stand in for a host that predates the rename.
    pub fn generate_named(name: &str, not_before: i64, not_after: i64) -> Result<Self, PinError> {
        let key = KeyPair::generate()?;
        Ok(Self {
            cert_der: issue(&key, name, not_before, not_after)?,
            key_der: key.serialize_der(),
        })
    }

    /// The same key under a new certificate that carries the current name and
    /// the window of this one. The pin is the SPKI, so devices that pinned the
    /// old certificate keep trusting the host.
    pub fn reissued(&self) -> Result<Self, PinError> {
        let key = KeyPair::try_from(self.key_der.as_slice())?;
        let validity = parse(&self.cert_der)?.validity().clone();
        Ok(Self {
            cert_der: issue(
                &key,
                CURRENT_NAME,
                validity.not_before.timestamp(),
                validity.not_after.timestamp(),
            )?,
            key_der: self.key_der.clone(),
        })
    }

    /// Whether the certificate carries the current common name. A host minted
    /// before the rename does not.
    pub fn carries_current_name(&self) -> Result<bool, PinError> {
        let certificate = parse(&self.cert_der)?;
        let common_name = certificate
            .subject()
            .iter_common_name()
            .next()
            .and_then(|name| name.as_str().ok());
        Ok(common_name == Some(CURRENT_COMMON_NAME))
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

/// A self-signed certificate for `key`, named `name`, valid in the given window.
fn issue(key: &KeyPair, name: &str, not_before: i64, not_after: i64) -> Result<Vec<u8>, PinError> {
    let mut params = CertificateParams::new(vec![name.to_string()])?;
    params
        .distinguished_name
        .push(DnType::CommonName, format!("{name} host"));
    params.not_before = datetime(not_before)?;
    params.not_after = datetime(not_after)?;
    Ok(params.self_signed(key)?.der().as_ref().to_vec())
}

fn parse(der: &[u8]) -> Result<X509Certificate<'_>, PinError> {
    X509Certificate::from_der(der)
        .map(|(_, certificate)| certificate)
        .map_err(|error| PinError::MalformedCertificate(format!("{error:?}")))
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

    #[test]
    fn a_reissued_identity_keeps_its_key_pin_and_window_and_takes_the_current_name() {
        let legacy =
            Identity::generate_named("herdr-notify", 1_000, 2_000_000_000).expect("legacy");
        assert!(!legacy.carries_current_name().expect("name"));
        let current = legacy.reissued().expect("reissue");
        assert!(current.carries_current_name().expect("name"));
        assert_eq!(current.key_der(), legacy.key_der());
        assert_eq!(
            current.fingerprint().expect("pin"),
            legacy.fingerprint().expect("pin")
        );
        assert_ne!(current.cert_der(), legacy.cert_der());
        let info = CertificateInfo::read(current.cert_der()).expect("read");
        assert!(info.check_at(1_500).is_ok());
    }
}
