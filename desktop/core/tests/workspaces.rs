mod common;

use std::sync::{Arc, Mutex};

use common::read_request;
use herdr_desktop_core::error::{CoreError, WriteAction};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

async fn one_shot(status: u16, response: &'static str) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let server_captured = captured.clone();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let (headers, body) = read_request(&mut socket).await;
        assert!(
            headers.starts_with("POST /v1/workspaces HTTP/1.1"),
            "{headers}"
        );
        *server_captured.lock().unwrap() = body;
        let reason = match status {
            200 => "OK",
            403 => "Forbidden",
            409 => "Conflict",
            422 => "Unprocessable Entity",
            _ => "Error",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let _ = socket.shutdown().await;
    });
    (format!("http://{address}"), captured)
}

fn client(base_url: &str) -> GatewayClient {
    GatewayClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

#[tokio::test]
async fn create_workspace_sends_agent_and_cwd_and_parses_the_pane() {
    let (base_url, captured) = one_shot(
        200,
        r#"{"ok":true,"workspace_id":"w1","pane_id":"w1:p4","agent":{"agent":"opencode","agent_status":"unknown","pane_id":"w1:p4"}}"#,
    )
    .await;

    let created = client(&base_url)
        .create_workspace("opencode", "D:/repo", Some("  Mi tarea  "))
        .await
        .unwrap();

    assert_eq!(created.workspace_id, "w1");
    assert_eq!(created.pane_id, "w1:p4");
    let body: Value = serde_json::from_slice(&captured.lock().unwrap()).unwrap();
    assert_eq!(body["agent"], "opencode");
    assert_eq!(body["cwd"], "D:/repo");
    assert_eq!(body["label"], "Mi tarea", "the label is trimmed");
}

#[tokio::test]
async fn an_empty_label_is_omitted() {
    let (base_url, captured) = one_shot(
        200,
        r#"{"ok":true,"workspace_id":"w1","pane_id":"w1:p4","agent":{}}"#,
    )
    .await;
    client(&base_url)
        .create_workspace("claude", "D:/repo", None)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&captured.lock().unwrap()).unwrap();
    assert!(body.get("label").is_none());
}

#[tokio::test]
async fn a_422_exposes_the_field_so_the_ui_can_explain_it() {
    let (base_url, _) = one_shot(
        422,
        r#"{"detail":{"code":"invalid_body","errors":[{"loc":["cwd"],"msg":"cwd is not a directory"}]}}"#,
    )
    .await;
    let error = client(&base_url)
        .create_workspace("opencode", "D:/nope", None)
        .await
        .unwrap_err();

    match &error {
        CoreError::Status {
            status,
            field,
            code,
            ..
        } => {
            assert_eq!(*status, 422);
            assert_eq!(field.as_deref(), Some("cwd"));
            assert_eq!(code.as_deref(), Some("invalid_body"));
        }
        other => panic!("expected a status error, got {other:?}"),
    }
    let info = error.describe(WriteAction::CreateWorkspace);
    assert!(info.message.contains("carpeta"), "{}", info.message);
}

#[tokio::test]
async fn a_403_names_the_respond_grant_for_creation() {
    let (base_url, _) = one_shot(403, r#"{"detail":"forbidden"}"#).await;
    let error = client(&base_url)
        .create_workspace("opencode", "D:/repo", None)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Forbidden));
    let info = error.describe(WriteAction::CreateWorkspace);
    assert!(info.message.contains("RESPOND"));
}

#[tokio::test]
async fn a_409_explains_that_the_host_could_not_start_the_agent() {
    let (base_url, _) = one_shot(409, r#"{"detail":"spawn failed: program not found"}"#).await;
    let error = client(&base_url)
        .create_workspace("ghost", "D:/repo", None)
        .await
        .unwrap_err();
    let info = error.describe(WriteAction::CreateWorkspace);
    assert!(info.message.contains("no pudo iniciar"), "{}", info.message);
    assert!(info.action.as_deref().unwrap().contains("agents.toml"));
}
