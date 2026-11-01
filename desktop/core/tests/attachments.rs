mod common;

use base64::{engine::general_purpose::STANDARD, Engine};
use common::spawn_server;
use herdr_desktop_core::chat::ChatFailure;
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;
use serde_json::{json, Value};

const CREATED: &str = r#"{"ok":true,"attachment":{"id":"att_01K","state":"uploading"}}"#;
const CHUNKED: &str = r#"{"ok":true,"next_offset":8}"#;
const COMPLETED: &str = r#"{"ok":true,"attachment":{"id":"att_01K","name":"captura.png","mime":"image/webp","size":48212,"w":1024,"h":768,"state":"ready"}}"#;
const SENT: &str = r#"{"ok":true,"message":{"id":"msg_01K","conversation_id":"cnv_01JABC","seq":4,"author_kind":"user","body_md":"(adjunto)","state":"complete","created_at":""},"turn":{"id":"run_01J","state":"queued","agent_id":"ag_01J"}}"#;
const OFFSET: &str = r#"{"detail":{"code":"offset_mismatch","message":"x","next_offset":131072}}"#;
const UNKNOWN: &str =
    r#"{"detail":{"code":"unknown_attachment","message":"x","field":"attachments"}}"#;

fn client(base_url: &str) -> GatewayClient {
    GatewayClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

#[tokio::test]
async fn an_upload_sends_the_contract_bodies_and_the_message_takes_the_file() {
    let (base_url, captured) = spawn_server(vec![
        (201, CREATED),
        (200, CHUNKED),
        (200, COMPLETED),
        (202, SENT),
    ])
    .await;
    let client = client(&base_url);
    let created = client
        .chat_attachment_create("cnv_01JABC", "a-1", "captura.png", 8, "9f86d081")
        .await
        .unwrap();
    assert_eq!(created.id, "att_01K");
    let next = client
        .chat_attachment_chunk("cnv_01JABC", "att_01K", 0, "iVBORw0K")
        .await
        .unwrap();
    assert_eq!(next, 8);
    let done = client
        .chat_attachment_complete("cnv_01JABC", "att_01K")
        .await
        .unwrap();
    assert_eq!(done.state.as_deref(), Some("ready"));
    client
        .chat_send_message("cnv_01JABC", "c-9", "", None, &["att_01K".to_string()])
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(
        captured[0].target,
        "/v1/chat/conversations/cnv_01JABC/attachments"
    );
    let create: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert_eq!(create["name"], "captura.png");
    assert_eq!(create["size"], 8);
    assert_eq!(
        captured[1].target,
        "/v1/chat/conversations/cnv_01JABC/attachments/att_01K/chunks"
    );
    let chunk: Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(chunk, json!({"offset": 0, "data": "iVBORw0K"}));
    assert_eq!(captured[2].method, "POST");
    let send: Value = serde_json::from_slice(&captured[3].body).unwrap();
    assert_eq!(send["text"], "");
    assert_eq!(send["attachments"], json!(["att_01K"]));
    assert!(send.get("reply_to").is_none());
}

#[tokio::test]
async fn a_send_without_text_or_files_is_refused_and_files_are_omitted_when_absent() {
    let (base_url, captured) = spawn_server(vec![(202, SENT)]).await;
    let client = client(&base_url);
    assert!(client
        .chat_send_message("cnv_01JABC", "c-1", "  ", None, &[])
        .await
        .is_err());
    client
        .chat_send_message("cnv_01JABC", "c-2", "Hola", None, &[])
        .await
        .unwrap();
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 1, "the empty send never leaves the machine");
    let body: Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert!(body.get("attachments").is_none());
}

#[tokio::test]
async fn an_offset_mismatch_keeps_the_point_to_resume_from() {
    let (base_url, _captured) = spawn_server(vec![(409, OFFSET)]).await;
    let failure = client(&base_url)
        .chat_attachment_chunk("cnv_01JABC", "att_01K", 0, "AAAA")
        .await
        .unwrap_err();
    match failure {
        ChatFailure::AttachmentRefused {
            code, next_offset, ..
        } => {
            assert_eq!(code, "offset_mismatch");
            assert_eq!(next_offset, Some(131_072));
        }
        other => panic!("esperaba attachment_refused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn a_refused_file_in_a_message_names_its_field() {
    let (base_url, _captured) = spawn_server(vec![(422, UNKNOWN)]).await;
    let failure = client(&base_url)
        .chat_send_message("cnv_01JABC", "c-3", "", None, &["att_01K".to_string()])
        .await
        .unwrap_err();
    match failure {
        ChatFailure::AttachmentRefused { code, field, .. } => {
            assert_eq!(code, "unknown_attachment");
            assert_eq!(field.as_deref(), Some("attachments"));
        }
        other => panic!("esperaba attachment_refused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn the_content_comes_back_as_base64_bytes_not_as_a_path() {
    let (base_url, captured) = spawn_server(vec![(200, "RIFFdata")]).await;
    let content = client(&base_url)
        .chat_attachment_content("cnv_01JABC", "att_01K")
        .await
        .unwrap();
    assert_eq!(content.mime, "image/webp");
    assert_eq!(content.data, STANDARD.encode(b"RIFFdata"));
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "GET");
    assert!(captured[0].target.ends_with("/att_01K/content"));
}
