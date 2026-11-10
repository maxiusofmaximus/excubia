//! The channel binding each connection hands the handlers (`PeerAddr::binding`, RFC 9266): the server and the
//! client of one TLS connection derive the same value, two connections derive different ones, and the plaintext
//! loopback listener has none.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ConnectInfo;
use axum::routing::get;
use axum::serve::Listener;
use axum::Router;
use excubia_core_api::{PeerAddr, TlsListener};
use excubia_core_pin::{client_config, server_config, Identity, PinSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

const REQUEST: &[u8] = b"GET /binding HTTP/1.1\r\nHost: lan\r\nConnection: close\r\n\r\n";

/// Answers with the binding the server saw for this connection, in hex, or `none`.
fn app() -> Router {
    Router::new().route(
        "/binding",
        get(|ConnectInfo(peer): ConnectInfo<PeerAddr>| async move {
            peer.binding.map_or_else(|| "none".to_owned(), hex::encode)
        }),
    )
}

fn body_of(response: &[u8]) -> String {
    let text = String::from_utf8_lossy(response);
    assert!(text.starts_with("HTTP/1.1 200"), "{text}");
    text.split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default()
}

async fn tls_server() -> (SocketAddr, PinSet) {
    let identity = Identity::generate().expect("identity");
    let pins = PinSet::new(identity.fingerprint().expect("fingerprint"), None);
    let config = Arc::new(server_config(&identity).expect("server config"));
    let listener = TlsListener::bind("127.0.0.1:0".parse().expect("address"), config)
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        let service = app().into_make_service_with_connect_info::<PeerAddr>();
        let _ = axum::serve(listener, service).await;
    });
    (addr, pins)
}

/// One request over a fresh TLS connection: what the server reported, and what the client derived itself.
async fn exchange(addr: SocketAddr, pins: &PinSet) -> (String, String) {
    let config = client_config(*pins).expect("client config");
    let tcp = TcpStream::connect(addr).await.expect("connect");
    let name = ServerName::try_from("excubia.lan")
        .expect("name")
        .to_owned();
    let mut tls = TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .expect("handshake");
    let mut mine = [0u8; 32];
    tls.get_ref()
        .1
        .export_keying_material(&mut mine, b"EXPORTER-Channel-Binding", None)
        .expect("client exporter");
    tls.write_all(REQUEST).await.expect("write");
    let mut response = Vec::new();
    // A peer that closes without close_notify still answered; keep what came.
    let _ = tls.read_to_end(&mut response).await;
    (body_of(&response), hex::encode(mine))
}

#[tokio::test(flavor = "multi_thread")]
async fn both_ends_of_one_connection_derive_the_same_binding() {
    let (addr, pins) = tls_server().await;
    let (server, client) = exchange(addr, &pins).await;
    assert_eq!(server.len(), 64, "{server}");
    assert_ne!(server, "00".repeat(32), "the exporter is not blank");
    assert_eq!(server, client);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_connections_have_different_bindings() {
    let (addr, pins) = tls_server().await;
    let (first, first_client) = exchange(addr, &pins).await;
    let (second, second_client) = exchange(addr, &pins).await;
    assert_ne!(first, second);
    assert_eq!((first, second), (first_client, second_client));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_plaintext_loopback_listener_has_no_binding() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        let service = app().into_make_service_with_connect_info::<PeerAddr>();
        let _ = axum::serve(listener, service).await;
    });
    let mut tcp = TcpStream::connect(addr).await.expect("connect");
    tcp.write_all(REQUEST).await.expect("write");
    let mut response = Vec::new();
    let _ = tcp.read_to_end(&mut response).await;
    assert_eq!(body_of(&response), "none");
}
