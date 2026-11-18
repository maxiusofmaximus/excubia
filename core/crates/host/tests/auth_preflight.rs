//! What the host answers before it reads a body: a declared size over the cap
//! is `413`, an untrusted device is `401`, and a signed body that stalls is
//! `408` at its read time. None of these may wait for the body to arrive.

mod harness;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use excubia_auth::canonical_string;
use excubia_test_support::DemoIdentity;
use harness::TestHost;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

/// The head of a signed request whose declared length is `body.len()`.
fn signed_head(method: &str, path: &str, identity: &DemoIdentity, body: &[u8]) -> Vec<u8> {
    let timestamp = now();
    let nonce = "bm9uY2Utc3RhbGw=";
    let message = canonical_string(method, path, body, timestamp, nonce).expect("canonical");
    let signature = identity.sign(&message);
    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n\
         X-Excubia-Device: {}\r\nX-Excubia-Timestamp: {timestamp}\r\n\
         X-Excubia-Nonce: {nonce}\r\nX-Excubia-Signature: {signature}\r\n\
         Content-Length: {}\r\n\r\n",
        identity.device_id(),
        body.len()
    )
    .into_bytes()
}

/// Sends `head` and the first bytes of the body, then keeps the socket open
/// and returns the status the server answers with. The server closes after
/// it answers, so the read ends there; `within` bounds the wait.
async fn status_for(host: &TestHost, head: &[u8], sent: &[u8], within: Duration) -> u16 {
    let mut stream = TcpStream::connect(host.addr).await.expect("connect");
    stream.write_all(head).await.expect("head");
    stream.write_all(sent).await.expect("sent");
    let mut response = Vec::new();
    tokio::time::timeout(within, stream.read_to_end(&mut response))
        .await
        .expect("the server answered without waiting for the body")
        .expect("read");
    String::from_utf8_lossy(&response)
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status line")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_from_an_unknown_device_is_401_without_being_read() {
    let host = TestHost::start("preflight-unknown").await;
    let ghost = DemoIdentity::generate("dev-ghost", "ghost");
    // The declared body never arrives: only the refusal can answer.
    let head = signed_head("POST", "/v1/mail/login/start", &ghost, b"{\"x\":1}");
    let status = status_for(&host, &head, b"{", Duration::from_secs(3)).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_from_a_revoked_device_is_401_without_being_read() {
    let host = TestHost::start("preflight-revoked").await;
    let (client, paired) = host.pair("revoked").await;
    host.devices.revoke(&paired.device_id).expect("revoke");
    let identity = client.identity().clone();
    let head = signed_head("POST", "/v1/mail/login/start", &identity, b"{\"x\":1}");
    let status = status_for(&host, &head, b"{", Duration::from_secs(3)).await;
    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversize_declared_body_is_401_for_a_stranger_and_413_for_a_trusted_device() {
    let host = TestHost::start("preflight-413").await;
    // No credentials: refused before the body is touched, so nothing is sent and the answer is at once.
    let stranger = b"POST /v1/mail/login/start HTTP/1.1\r\nHost: localhost\r\n\
                     Content-Length: 300000\r\n\r\n";
    let status = status_for(&host, stranger, b"", Duration::from_secs(3)).await;
    assert_eq!(status, 401);

    // A trusted device sends its whole 300 000-byte body (over the 256 KiB cap): the drained body gets its 413.
    let (client, _paired) = host.pair("oversize").await;
    let identity = client.identity().clone();
    let big = vec![b'x'; 300_000];
    let head = signed_head("POST", "/v1/mail/login/start", &identity, &big);
    let status = status_for(&host, &head, &big, Duration::from_secs(10)).await;
    assert_eq!(status, 413);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signed_body_that_stalls_is_408_at_its_read_time() {
    let host = TestHost::start("preflight-408").await;
    let (client, _paired) = host.pair("stalled").await;
    let identity = client.identity().clone();
    // Signed as the full ten bytes; only three ever arrive. The ten-second read time ends the wait.
    let full = b"0123456789";
    let head = signed_head("POST", "/v1/mail/login/start", &identity, full);
    let status = status_for(&host, &head, b"012", Duration::from_secs(20)).await;
    assert_eq!(status, 408);
}
