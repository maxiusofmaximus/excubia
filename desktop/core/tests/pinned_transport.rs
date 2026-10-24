//! The pin table (CORE11B): a real TLS 1.3 server with the host's identity,
//! dialed through the desktop's pinned transport.

use std::net::SocketAddr;
use std::sync::Arc;

use herdr_desktop_core::client::{pair_device, PairResponse};
use herdr_desktop_core::pair_link::{pair_through_link, PairLink};
use herdr_desktop_core::transport::pinned_base_url;
use herdr_desktop_core::{CoreError, Transport};
use hn_core_pin::{unix_now, Identity, PinSet};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::version::TLS12;
use rustls::ServerConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const YEAR: i64 = 365 * 24 * 60 * 60;
const BODY: &str = r#"{"device_id":"dev-tls","label":"Escritorio","permissions":["read_state"]}"#;

/// A host that presents `identity` over TLS 1.3 and answers every request
/// with the same pairing response.
async fn spawn_host(identity: &Identity) -> SocketAddr {
    spawn_with(hn_core_pin::server_config(identity).unwrap()).await
}

async fn spawn_with(config: ServerConfig) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut stream) = acceptor.accept(socket).await {
                    read_request(&mut stream).await;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{BODY}",
                        BODY.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                }
            });
        }
    });
    address
}

async fn read_request<S: AsyncReadExt + Unpin>(stream: &mut S) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk).await.unwrap();
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_ascii_lowercase();
    let length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + length {
        let read = stream.read(&mut chunk).await.unwrap();
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn pin_of(identity: &Identity) -> PinSet {
    PinSet::new(identity.fingerprint().unwrap(), None)
}

async fn pair_pinned(address: SocketAddr, pins: PinSet) -> Result<PairResponse, CoreError> {
    pair_device(
        &pinned_base_url(address),
        &Transport::Pinned(pins),
        "p256",
        "BJk/KOnrA7maxRF7PgLwQTestKey=",
        "Escritorio",
        "KEQTX5Q",
    )
    .await
}

#[tokio::test]
async fn the_pinned_key_is_accepted() {
    let identity = Identity::generate().unwrap();
    let address = spawn_host(&identity).await;

    let response = pair_pinned(address, pin_of(&identity)).await.unwrap();

    assert_eq!(response.device_id, "dev-tls");
}

#[tokio::test]
async fn a_different_certificate_is_refused_as_a_pin_mismatch() {
    let pinned = Identity::generate().unwrap();
    let other = Identity::generate().unwrap();
    let address = spawn_host(&other).await;

    let error = pair_pinned(address, pin_of(&pinned)).await.unwrap_err();

    assert!(matches!(error, CoreError::PinMismatch), "{error:?}");
}

#[tokio::test]
async fn a_rotation_accepts_either_pin() {
    let old = Identity::generate().unwrap();
    let new = Identity::generate().unwrap();
    let stranger = Identity::generate().unwrap();
    let pins = PinSet::new(old.fingerprint().unwrap(), Some(new.fingerprint().unwrap()));

    assert!(pair_pinned(spawn_host(&old).await, pins).await.is_ok());
    assert!(pair_pinned(spawn_host(&new).await, pins).await.is_ok());
    assert!(matches!(
        pair_pinned(spawn_host(&stranger).await, pins).await,
        Err(CoreError::PinMismatch)
    ));
}

#[tokio::test]
async fn an_expired_certificate_is_refused_even_when_pinned() {
    let now = unix_now();
    let identity = Identity::generate_within(now - 2 * YEAR, now - YEAR).unwrap();

    let error = pair_pinned(spawn_host(&identity).await, pin_of(&identity))
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::CertificateExpired), "{error:?}");
}

#[tokio::test]
async fn a_certificate_not_yet_valid_is_refused_even_when_pinned() {
    let now = unix_now();
    let identity = Identity::generate_within(now + YEAR, now + 2 * YEAR).unwrap();

    let error = pair_pinned(spawn_host(&identity).await, pin_of(&identity))
        .await
        .unwrap_err();

    assert!(
        matches!(error, CoreError::CertificateNotYetValid),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_tls_12_only_host_is_refused() {
    let identity = Identity::generate().unwrap();
    let certificate = CertificateDer::from(identity.cert_der().to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der().to_vec()));
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&TLS12])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .unwrap();

    let error = pair_pinned(spawn_with(config).await, pin_of(&identity))
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::Transport(_)), "{error:?}");
}

#[tokio::test]
async fn a_closed_port_is_an_ordinary_transport_failure() {
    let address = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let identity = Identity::generate().unwrap();

    let error = pair_pinned(address, pin_of(&identity)).await.unwrap_err();

    assert!(matches!(error, CoreError::Transport(_)), "{error:?}");
}

#[tokio::test]
async fn a_link_moves_past_a_stranger_to_the_pinned_host() {
    let pinned = Identity::generate().unwrap();
    let stranger = Identity::generate().unwrap();
    let link = PairLink {
        code: "KEQTX5Q".to_string(),
        pins: pin_of(&pinned),
        addresses: vec![spawn_host(&stranger).await, spawn_host(&pinned).await],
        expires_at: unix_now() + 600,
    };
    let host_address = link.addresses[1];

    let paired = pair_through_link(&link, "BJk/KOnrA7maxRF7PgLwQTestKey=", "Escritorio")
        .await
        .unwrap();

    assert_eq!(paired.base_url, pinned_base_url(host_address));
    assert_eq!(paired.response.device_id, "dev-tls");
}

#[tokio::test]
async fn a_link_whose_addresses_all_fail_the_pin_says_so() {
    let pinned = Identity::generate().unwrap();
    let stranger = Identity::generate().unwrap();
    let link = PairLink {
        code: "KEQTX5Q".to_string(),
        pins: pin_of(&pinned),
        addresses: vec![spawn_host(&stranger).await],
        expires_at: unix_now() + 600,
    };

    let error = pair_through_link(&link, "key", "Escritorio")
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::PinMismatch), "{error:?}");
}
