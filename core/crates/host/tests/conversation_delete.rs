//! Deleting a conversation over HTTP (TASK-CONV-BORRAR): the signed `RESPOND`
//! grant is needed, an open turn answers `409 conversation_busy` and keeps the
//! conversation, and the delete answers `200` once and `404` after that.

mod harness;

use std::time::Duration;

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use serde_json::Value;

const TTL: Duration = Duration::from_secs(45);

fn detail(body: &str) -> Value {
    let parsed: Value = serde_json::from_str(body).expect("json error body");
    parsed["detail"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delete_needs_a_signature_and_respond_and_waits_for_an_open_turn() {
    // A silent seat keeps the turn queued, so the conversation is busy until a stop.
    let host = TestHost::start_with_seat("conv-delete", TTL, SeatKind::Silent).await;
    let (owner, paired) = host.pair("Desktop borrar").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let reader = host
        .paired_with("Solo lectura", &[Permission::ReadState])
        .await;

    let created = owner.chat_create("Borrar", None).await.expect("create");
    let conversation_id = created["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let path = format!("/v1/chat/conversations/{conversation_id}");

    let unsigned = reqwest::Client::new()
        .delete(format!("{}{path}", host.base_url()))
        .send()
        .await
        .expect("unsigned");
    assert_eq!(unsigned.status().as_u16(), 401);

    let (status, body) = reader.request("DELETE", &path, None).await.expect("reader");
    assert_eq!(status, 403, "{body}");

    owner
        .chat_send(&conversation_id, "borrar-1", "hola")
        .await
        .expect("send");
    let (status, body) = owner.request("DELETE", &path, None).await.expect("busy");
    assert_eq!(status, 409, "{body}");
    assert_eq!(detail(&body), "conversation_busy");
    owner
        .chat_conversation(&conversation_id)
        .await
        .expect("the busy conversation is kept");

    let stop = format!("{path}/stop");
    let (status, body) = owner.request("POST", &stop, None).await.expect("stop");
    assert_eq!(status, 200, "{body}");

    let (status, body) = owner.request("DELETE", &path, None).await.expect("delete");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("json")["ok"],
        true
    );

    let (status, body) = owner.request("DELETE", &path, None).await.expect("again");
    assert_eq!(status, 404, "{body}");
    assert_eq!(detail(&body), "no_such_conversation");
}
