//! Raw HTTP on the test host: the hook routes are not signed, so the tests
//! speak HTTP/1.1 themselves and read the answer verbatim.

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Send one raw request and read the whole answer.
pub async fn raw_exchange(addr: SocketAddr, request: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request).await.expect("write");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("read");
    let text = String::from_utf8_lossy(&response).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status");
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

/// The body the plugin sends to open an approval.
pub fn approval_body(action: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "kind": "command",
        "tool": "bash",
        "summary": format!("Ejecutar comando: {action}"),
        "cwd": "/work",
        "paths": [],
        "action_digest": format!("sha256:{}", "0".repeat(64)),
        "payload_redacted": { "command": action },
        "ttl_seconds": 120,
    }))
    .expect("json")
}

/// The `detail` of an API error body.
pub fn detail(body: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(body).expect("json body")["detail"].clone()
}
