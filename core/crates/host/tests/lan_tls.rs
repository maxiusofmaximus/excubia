//! The LAN TLS listener, end to end: a device pinned to the host's certificate
//! reaches the device routes over TLS 1.3, and never reaches a hook route.
//! A different certificate and plaintext are refused at the handshake.

mod harness;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::serve::Listener;
use harness::TestHost;
use hn_core_api::{serve_client, AgentActions, ApiConfig, ApiContext, TlsListener};
use hn_core_host::tls_slots::TlsSlots;
use hn_core_pin::{client_config, Identity, PinSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

/// Serve the device routes on a LAN-style TLS listener over the harness host's
/// state, with the certificate the host's slots hold.
async fn lan_listener(host: &TestHost) -> (SocketAddr, JoinHandle<std::io::Result<()>>) {
    let identity = TlsSlots::open(&host.data_dir).active().expect("identity");
    let config = Arc::new(hn_core_pin::server_config(&identity).expect("server config"));
    let listener = TlsListener::bind("127.0.0.1:0".parse().expect("address"), config)
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local address");
    let actions: Arc<dyn AgentActions> = Arc::clone(&host.host) as Arc<dyn AgentActions>;
    let context = ApiContext::new(ApiConfig {
        store: Arc::clone(&host.store),
        actions,
        devices: Arc::clone(&host.devices),
        agents: Arc::new(hn_core_host::load_agents(&host.data_dir).expect("catalog")),
        chat: Arc::clone(&host.chat),
        seat: Arc::clone(&host.seat),
        hook_token: Arc::new(host.hook_token.clone()),
    });
    (addr, tokio::spawn(serve_client(listener, context)))
}

/// One request over TLS, pinned to `pins`. Returns what the peer sent, or the
/// handshake error.
async fn exchange(addr: SocketAddr, pins: PinSet, request: &str) -> std::io::Result<String> {
    let config = client_config(pins).expect("client config");
    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(addr).await?;
    let name = ServerName::try_from("herdr-notify.lan")
        .expect("name")
        .to_owned();
    let mut tls = connector.connect(name, tcp).await?;
    tls.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    // A peer that closes without close_notify still answered; keep what came.
    let _ = tls.read_to_end(&mut response).await;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_device_reaches_the_device_routes_over_tls() {
    let host = TestHost::start("lan-tls").await;
    let (addr, task) = lan_listener(&host).await;
    let pins = TlsSlots::open(&host.data_dir).pins().expect("pins");

    let health = exchange(
        addr,
        pins,
        "GET /v1/health HTTP/1.1\r\nHost: lan\r\nConnection: close\r\n\r\n",
    )
    .await
    .expect("the pinned handshake succeeds");
    assert!(health.starts_with("HTTP/1.1 200"), "{health}");

    task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lan_listener_has_no_hook_routes() {
    let host = TestHost::start("lan-tls-hooks").await;
    let (addr, task) = lan_listener(&host).await;
    let pins = TlsSlots::open(&host.data_dir).pins().expect("pins");

    // The host token is valid, and the route still does not exist here.
    let body = r#"{"pane_id":"w1:p1","state":"working"}"#;
    let request = format!(
        "POST /v1/hooks/report HTTP/1.1\r\nHost: lan\r\nX-HN-Hook-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        host.hook_token,
        body.len()
    );
    let response = exchange(addr, pins, &request).await.expect("handshake");
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");

    task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_pinned_another_certificate_is_refused_at_the_handshake() {
    let host = TestHost::start("lan-tls-wrong-pin").await;
    let (addr, task) = lan_listener(&host).await;
    let other = Identity::generate().expect("another identity");
    let wrong = PinSet::new(other.fingerprint().expect("fingerprint"), None);

    let error = exchange(addr, wrong, "GET /v1/health HTTP/1.1\r\n\r\n")
        .await
        .expect_err("an unpinned certificate must not connect");
    assert!(error.to_string().contains("Unpinned"), "{error}");

    task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn plaintext_is_refused_on_the_lan_port() {
    let host = TestHost::start("lan-tls-plain").await;
    let (addr, task) = lan_listener(&host).await;

    let mut tcp = TcpStream::connect(addr).await.expect("connect");
    tcp.write_all(b"GET /v1/health HTTP/1.1\r\nHost: lan\r\n\r\n")
        .await
        .expect("write");
    let mut response = Vec::new();
    let _ = tcp.read_to_end(&mut response).await;
    let text = String::from_utf8_lossy(&response);
    assert!(
        !text.contains("HTTP/1.1"),
        "plaintext got an answer: {text}"
    );

    task.abort();
}
