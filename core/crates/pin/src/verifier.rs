use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, DigitallySignedStruct, OtherError, PeerIncompatible, SignatureScheme,
};

use crate::certificate::CertificateInfo;
use crate::{PinError, PinSet};

/// Accepts a server only when its leaf certificate's key is pinned, the
/// certificate is inside its validity window, and the handshake proves the
/// peer holds that key. Trust comes from the pin alone: no CA, no hostname.
#[derive(Debug)]
pub struct PinVerifier {
    pins: PinSet,
    provider: Arc<CryptoProvider>,
}

impl PinVerifier {
    pub fn new(pins: PinSet, provider: Arc<CryptoProvider>) -> Self {
        Self { pins, provider }
    }

    fn check(&self, end_entity: &CertificateDer<'_>, now: UnixTime) -> Result<(), PinError> {
        let info = CertificateInfo::read(end_entity)?;
        info.check_at(now.as_secs() as i64)?;
        if self.pins.contains(&info.fingerprint()) {
            Ok(())
        } else {
            Err(PinError::Unpinned)
        }
    }
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity, now)
            .map(|()| ServerCertVerified::assertion())
            .map_err(to_rustls)
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            PeerIncompatible::Tls12NotOfferedOrEnabled,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Keep the typed reason: rustls carries it through the handshake error.
fn to_rustls(error: PinError) -> rustls::Error {
    let certificate = match error {
        PinError::NotYetValid => CertificateError::NotValidYet,
        PinError::Expired => CertificateError::Expired,
        other => CertificateError::Other(OtherError(Arc::new(other))),
    };
    rustls::Error::InvalidCertificate(certificate)
}
