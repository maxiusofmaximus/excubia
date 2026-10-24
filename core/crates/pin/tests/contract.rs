//! The pinning contract, end to end: real TLS handshakes on loopback between a
//! host identity and a device configuration, one row per case of CORE11.

use std::io;
use std::sync::Arc;

use hn_core_pin::{
    client_config, server_config, unix_now, CertificateInfo, Fingerprint, Identity, PinError,
    PinSet, PinVerifier,
};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::ServerName;
use rustls::version::TLS12;
use rustls::{AlertDescription, CertificateError, ClientConfig, OtherError};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

const YEAR: i64 = 365 * 24 * 60 * 60;

/// Run one handshake: the host presents `server`, the device verifies with
/// `client`. The outcome is the device's: `Ok` only if its handshake completes.
async fn handshake(server: &Identity, client: ClientConfig) -> io::Result<()> {
    let config = server_config(server).expect("server config");
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let serving = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await?;
        acceptor.accept(tcp).await.map(|_| ())
    });
    let tcp = TcpStream::connect(address).await?;
    let name = ServerName::try_from("127.0.0.1").expect("server name");
    let outcome = TlsConnector::from(Arc::new(client))
        .connect(name, tcp)
        .await
        .map(|_| ());
    serving.abort();
    outcome
}

fn pinned(pins: PinSet) -> ClientConfig {
    client_config(pins).expect("client config")
}

/// A device that offers only TLS 1.2, with the same pin rule.
fn tls12_only(pins: PinSet) -> ClientConfig {
    let provider = Arc::new(default_provider());
    let verifier = PinVerifier::new(pins, Arc::clone(&provider));
    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS12])
        .expect("TLS 1.2 versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth()
}

/// The rustls error a refused handshake carries, typed.
fn refusal(outcome: io::Result<()>) -> rustls::Error {
    let error = outcome.expect_err("the handshake must be refused");
    *error
        .into_inner()
        .expect("a wrapped error")
        .downcast::<rustls::Error>()
        .expect("a rustls error")
}

fn certificate_refusal(outcome: io::Result<()>) -> CertificateError {
    match refusal(outcome) {
        rustls::Error::InvalidCertificate(certificate) => certificate,
        other => panic!("expected a certificate refusal, got {other}"),
    }
}

#[tokio::test]
async fn a_pinned_key_is_accepted() {
    let host = Identity::generate().expect("identity");
    let pins = PinSet::new(host.fingerprint().expect("pin"), None);
    handshake(&host, pinned(pins)).await.expect("accepted");
}

#[tokio::test]
async fn a_new_certificate_is_refused_under_the_old_pin() {
    let old = Identity::generate().expect("identity");
    let new = Identity::generate().expect("identity");
    let pins = PinSet::new(old.fingerprint().expect("pin"), None);
    match certificate_refusal(handshake(&new, pinned(pins)).await) {
        CertificateError::Other(OtherError(inner)) => {
            assert!(matches!(
                inner.downcast_ref::<PinError>(),
                Some(PinError::Unpinned)
            ))
        }
        other => panic!("expected an unpinned key, got {other}"),
    }
}

#[tokio::test]
async fn a_rotation_accepts_either_pin() {
    let current = Identity::generate().expect("identity");
    let next = Identity::generate().expect("identity");
    let pins = PinSet::new(
        current.fingerprint().expect("pin"),
        Some(next.fingerprint().expect("pin")),
    );
    handshake(&current, pinned(pins)).await.expect("current");
    handshake(&next, pinned(pins)).await.expect("next");
}

#[tokio::test]
async fn an_expired_certificate_is_refused() {
    let now = unix_now();
    let host = Identity::generate_within(now - 2 * YEAR, now - YEAR).expect("identity");
    let pins = PinSet::new(host.fingerprint().expect("pin"), None);
    let refusal = certificate_refusal(handshake(&host, pinned(pins)).await);
    assert!(matches!(refusal, CertificateError::Expired), "{refusal}");
}

#[tokio::test]
async fn a_certificate_not_yet_valid_is_refused() {
    let now = unix_now();
    let host = Identity::generate_within(now + YEAR, now + 2 * YEAR).expect("identity");
    let pins = PinSet::new(host.fingerprint().expect("pin"), None);
    let refusal = certificate_refusal(handshake(&host, pinned(pins)).await);
    assert!(
        matches!(refusal, CertificateError::NotValidYet),
        "{refusal}"
    );
}

#[tokio::test]
async fn tls_12_is_refused_by_the_host() {
    let host = Identity::generate().expect("identity");
    let pins = PinSet::new(host.fingerprint().expect("pin"), None);
    let refusal = refusal(handshake(&host, tls12_only(pins)).await);
    assert!(
        matches!(
            refusal,
            rustls::Error::AlertReceived(AlertDescription::ProtocolVersion)
        ),
        "{refusal}"
    );
}

#[test]
fn a_malformed_certificate_is_a_typed_error() {
    assert!(matches!(
        CertificateInfo::read(b"not a certificate"),
        Err(PinError::MalformedCertificate(_))
    ));
}

#[test]
fn a_malformed_fingerprint_is_a_typed_error() {
    assert!(matches!(
        "not-a-pin".parse::<Fingerprint>(),
        Err(PinError::MalformedFingerprint)
    ));
}
