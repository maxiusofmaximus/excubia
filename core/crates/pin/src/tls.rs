use std::sync::Arc;

use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::version::TLS13;
use rustls::{ClientConfig, ServerConfig};

use crate::{Identity, PinError, PinSet, PinVerifier};

/// The client side: TLS 1.3 only, the server certificate verified by pin.
pub fn client_config(pins: PinSet) -> Result<ClientConfig, PinError> {
    let provider = Arc::new(default_provider());
    let verifier = PinVerifier::new(pins, Arc::clone(&provider));
    Ok(ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

/// The host side: TLS 1.3 only, presenting the identity's self-signed leaf.
pub fn server_config(identity: &Identity) -> Result<ServerConfig, PinError> {
    let chain = vec![CertificateDer::from(identity.cert_der().to_vec())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der().to_vec()));
    ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_protocol_versions(&[&TLS13])?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(PinError::from)
}
