mod common;

use common::spawn_server;
use herdr_desktop_core::chat::{ChatFailure, ListFilter};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;

const SENT: &str = r#"{"ok":true,"message":{"id":"msg_01K","conversation_id":"cnv_01JABC","seq":4,"author_kind":"user","body_md":"Sí","state":"complete","created_at":""},"turn":{"id":"run_01J","state":"queued","agent_id":"ag_01J"}}"#;

fn client(base_url: &str) -> GatewayClient {
    GatewayClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

#[tokio::test]
async fn reply_to_is_sent_only_when_the_operator_answers_a_message() {
    let (base_url, captured) = spawn_server(vec![(202, SENT), (202, SENT)]).await;
    let client = client(&base_url);
    client.chat_send("cnv_01JABC", "c-1", "Sí").await.unwrap();
    client
        .chat_send_reply("cnv_01JABC", "c-2", "Sí", Some("msg_01K"))
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    let plain: serde_json::Value = serde_json::from_slice(&captured[0].body).unwrap();
    assert!(
        plain.get("reply_to").is_none(),
        "no reply_to without an answer"
    );
    let reply: serde_json::Value = serde_json::from_slice(&captured[1].body).unwrap();
    assert_eq!(
        reply["reply_to"],
        serde_json::json!({"kind": "message", "id": "msg_01K"})
    );
    assert_eq!(reply["text"], "Sí");
}

#[tokio::test]
async fn each_history_chip_asks_the_host_for_its_own_flag() {
    let empty = r#"{"ok":true,"conversations":[]}"#;
    let (base_url, captured) = spawn_server(vec![(200, empty), (200, empty), (200, empty)]).await;
    let client = client(&base_url);
    client
        .chat_conversations(None, None, false, false, Some(ListFilter::Attention))
        .await
        .unwrap();
    client
        .chat_conversations(None, None, false, false, Some(ListFilter::Working))
        .await
        .unwrap();
    client
        .chat_conversations(None, None, false, false, None)
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert!(captured[0].target.contains("attention=1"));
    assert!(!captured[0].target.contains("working"));
    assert!(captured[1].target.contains("working=1"));
    assert!(!captured[1].target.contains("attention"));
    assert!(!captured[2].target.contains("attention") && !captured[2].target.contains("working"));
}

#[tokio::test]
async fn counts_are_read_and_an_older_host_answers_none() {
    let counts = r#"{"ok":true,"all":5,"attention":2,"working":1,"archived":3}"#;
    let (base_url, captured) = spawn_server(vec![(200, counts)]).await;
    let read = client(&base_url)
        .chat_counts()
        .await
        .unwrap()
        .expect("counts");
    assert_eq!(
        (read.all, read.attention, read.working, read.archived),
        (5, 2, 1, 3)
    );
    assert_eq!(
        captured.lock().unwrap()[0].target,
        "/v1/chat/conversations/counts"
    );

    let (base_url, _) = spawn_server(vec![(404, r#"{"detail":"not found"}"#)]).await;
    assert!(client(&base_url).chat_counts().await.unwrap().is_none());

    let (base_url, _) = spawn_server(vec![(405, "")]).await;
    assert!(client(&base_url).chat_counts().await.unwrap().is_none());

    let (base_url, _) = spawn_server(vec![(403, r#"{"detail":"forbidden"}"#)]).await;
    assert!(
        client(&base_url).chat_counts().await.is_err(),
        "a 403 is not an older host"
    );
}

#[tokio::test]
async fn a_reply_to_a_missing_message_is_its_own_refusal() {
    let refusal = r#"{"detail":{"code":"unknown_reply_to","field":"reply_to","message":"no"}}"#;
    let (base_url, _) = spawn_server(vec![(422, refusal)]).await;
    let error = client(&base_url)
        .chat_send_reply("cnv_01JABC", "c-3", "Sí", Some("msg_01X"))
        .await
        .unwrap_err();
    assert!(
        matches!(error, ChatFailure::UnknownReplyTo { .. }),
        "llegó {error:?}"
    );
}
