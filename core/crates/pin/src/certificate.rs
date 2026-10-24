use x509_parser::certificate::X509Certificate;
use x509_parser::prelude::FromDer;

use crate::{Fingerprint, PinError};

/// What a verifier needs from one DER certificate: the key it pins and the
/// validity window it must sit inside. Everything else in it is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CertificateInfo {
    fingerprint: Fingerprint,
    not_before: i64,
    not_after: i64,
}

impl CertificateInfo {
    pub fn read(der: &[u8]) -> Result<Self, PinError> {
        let (rest, certificate) = X509Certificate::from_der(der)
            .map_err(|error| PinError::MalformedCertificate(format!("{error:?}")))?;
        if !rest.is_empty() {
            return Err(PinError::MalformedCertificate(
                "bytes follow the certificate".to_string(),
            ));
        }
        let validity = certificate.validity();
        Ok(Self {
            fingerprint: Fingerprint::of_spki(certificate.public_key().raw),
            not_before: validity.not_before.timestamp(),
            not_after: validity.not_after.timestamp(),
        })
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Inclusive at both ends, so a certificate is valid for its whole window.
    pub fn check_at(&self, now: i64) -> Result<(), PinError> {
        if now < self.not_before {
            Err(PinError::NotYetValid)
        } else if now > self.not_after {
            Err(PinError::Expired)
        } else {
            Ok(())
        }
    }
}
