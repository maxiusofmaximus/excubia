mod common;

use std::sync::Arc;

use common::read_request;
use herdr_desktop_core::client::pair_device;
use herdr_desktop_core::Transport;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// One-shot fake gateway: captures the exact request body and answers a pair
/// response, so the wire contract is asserted without a real server.
async fn spawn_pair_server() -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let server_captured = captured.clone();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let (headers, body) = read_request(&mut socket).await;
        assert!(headers.starts_with("POST /v1/pair HTTP/1.1"), "{headers}");
        *server_captured.lock().await = body;

        let response_body = r#"{"ok":true,"device_id":"dev-fake","curve":"p256","label":"Escritorio Prueba","permissions":["read_state"],"owner_id":"1","recovery_code":"AAAA-BBBB-CCCC-DDDD"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });

    (format!("http://{address}"), captured)
}

#[tokio::test]
async fn pairing_sends_the_label_and_code_and_parses_the_response() {
    let (base_url, captured) = spawn_pair_server().await;

    let response = pair_device(
        &base_url,
        &Transport::Plain,
        "p256",
        "BJk/KOnrA7maxRF7PgLwQTestKey=",
        "  Escritorio Prueba  ",
        " KEQTX5Q ",
    )
    .await
    .unwrap();

    assert_eq!(response.device_id, "dev-fake");
    assert_eq!(response.label.as_deref(), Some("Escritorio Prueba"));
    assert_eq!(response.permissions, vec!["read_state"]);
    assert_eq!(
        response.recovery_code.as_deref(),
        Some("AAAA-BBBB-CCCC-DDDD")
    );

    let body: Value = serde_json::from_slice(&captured.lock().await).unwrap();
    assert_eq!(body["curve"], "p256");
    assert_eq!(body["public_key"], "BJk/KOnrA7maxRF7PgLwQTestKey=");
    assert_eq!(body["label"], "Escritorio Prueba", "the label is trimmed");
    assert_eq!(body["code"], "KEQTX5Q", "the code is trimmed");
}

#[tokio::test]
async fn an_empty_code_is_omitted_from_the_pairing_body() {
    let (base_url, captured) = spawn_pair_server().await;

    pair_device(
        &base_url,
        &Transport::Plain,
        "p256",
        "key",
        "Escritorio",
        "",
    )
    .await
    .unwrap();

    let body: Value = serde_json::from_slice(&captured.lock().await).unwrap();
    assert!(body.get("code").is_none(), "no code means the tailnet path");
    assert_eq!(body["label"], "Escritorio");
}
