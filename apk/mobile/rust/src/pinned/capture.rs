//! The verifier of the first connection of a PAKE pairing, when there is no pin
//! yet. It accepts the leaf in its validity window and verifies the TLS 1.3
//! signature (the peer really holds that key), but trusts nothing: the pin is
//! born from the SPKI the host then confirms with the PAKE.
//!
//! Safe to use here because that connection carries only the SPAKE2 message,
//! which is useless without the code, and the host's confirmation tag binds the
//! exporter and the SPKI of this very connection: a man in the middle holds two
//! TLS sessions, so the tag fails and the device sends nothing else.

use std::sync::Arc;

use hn_core_pin::{CertificateInfo, PinError};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring::default_provider;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::version::TLS13;
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, PeerIncompatible, SignatureScheme,
};

use crate::api::pinned::{PinnedFailure, PinnedFailureKind as Kind};

#[derive(Debug)]
struct CaptureVerifier {
    provider: Arc<CryptoProvider>,
}

/// TLS 1.3 only, like `client_config`, with the capture verifier instead of a pin.
pub(crate) fn capture_config() -> Result<ClientConfig, PinnedFailure> {
    let provider = Arc::new(default_provider());
    let verifier = CaptureVerifier {
        provider: Arc::clone(&provider),
    };
    Ok(ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|error| PinnedFailure::of(Kind::Protocol, error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let window = CertificateInfo::read(end_entity)
            .and_then(|info| info.check_at(now.as_secs() as i64))
            .map_err(invalid_certificate);
        window.map(|()| ServerCertVerified::assertion())
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

fn invalid_certificate(error: PinError) -> rustls::Error {
    rustls::Error::InvalidCertificate(match error {
        PinError::NotYetValid => CertificateError::NotValidYet,
        PinError::Expired => CertificateError::Expired,
        _ => CertificateError::BadEncoding,
    })
}
