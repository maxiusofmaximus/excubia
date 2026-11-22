//! Deleting several conversations in one request (TASK-BULK1): `POST
//! /v1/chat/conversations/delete` answers each id, a busy or missing id does not stop
//! the others, a malformed body is `400 invalid_body`, one request is one write of the
//! budget, and the audit file gets one line with the counts.

mod harness;

use std::time::Duration;

use excubia_auth::Permission;
use excubia_chat::NewConversation;
use excubia_host::audit::AUDIT_FILE;
use harness::{SeatKind, TestHost};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);
const BULK: &str = "/v1/chat/conversations/delete";

fn bulk_body(ids: &[&str]) -> Option<Vec<u8>> {
    Some(serde_json::to_vec(&json!({ "ids": ids })).expect("json"))
}

fn parse(body: &str) -> Value {
    serde_json::from_str(body).expect("json body")
}

fn audit_lines(host: &TestHost) -> Vec<Value> {
    let text = std::fs::read_to_string(host.data_dir.join(AUDIT_FILE)).unwrap_or_default();
    text.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn three_conversations_go_in_one_request_and_duplicates_are_ignored() {
    let host = TestHost::start("bulk-delete-three").await;
    let (owner, paired) = host.pair("Desktop borrar varias").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let mut ids = Vec::new();
    for title in ["Uno", "Dos", "Tres"] {
        let created = owner.chat_create(title, None).await.expect("create");
        ids.push(
            created["conversation"]["id"]
                .as_str()
                .expect("id")
                .to_string(),
        );
    }
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut with_duplicate = refs.clone();
    with_duplicate.push(refs[0]);

    let (status, text) = owner
        .request("POST", BULK, bulk_body(&with_duplicate))
        .await
        .expect("bulk");

    assert_eq!(status, 200, "{text}");
    let answer = parse(&text);
    assert_eq!(answer["ok"], true);
    assert_eq!(
        answer["deleted"],
        json!(refs),
        "in the order sent, once each"
    );
    assert_eq!(answer["failed"], json!([]));
    for id in &ids {
        let (status, _) = owner
            .request("GET", &format!("/v1/chat/conversations/{id}"), None)
            .await
            .expect("show");
        assert_eq!(status, 404, "{id} is gone");
    }
    let audit = audit_lines(&host);
    let line = audit
        .iter()
        .find(|line| line["action"] == "conversations_delete")
        .expect("one audit line");
    assert_eq!(line["device"], paired.device_id.as_str());
    assert_eq!(line["detail"]["deleted"], "3");
    assert_eq!(line["detail"]["failed"], "0");
    assert!(!line.to_string().contains("Uno"), "no title in the audit");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_or_missing_id_fails_alone_and_the_rest_still_go() {
    // A silent seat keeps the turn queued, so the conversation stays busy.
    let host = TestHost::start_with_seat("bulk-delete-busy", TTL, SeatKind::Silent).await;
    let (owner, paired) = host.pair("Desktop ocupado").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let idle_a = owner.chat_create("Libre A", None).await.expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let busy = owner.chat_create("Ocupada", None).await.expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let idle_b = owner.chat_create("Libre B", None).await.expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    owner
        .chat_send(&busy, "bulk-busy-1", "hola")
        .await
        .expect("send");
    let missing = "cnv_01J00000000000000000000000";

    let (status, text) = owner
        .request(
            "POST",
            BULK,
            bulk_body(&[idle_a.as_str(), busy.as_str(), missing, idle_b.as_str()]),
        )
        .await
        .expect("bulk");

    assert_eq!(status, 200, "{text}");
    let answer = parse(&text);
    assert_eq!(answer["deleted"], json!([idle_a, idle_b]));
    assert_eq!(
        answer["failed"],
        json!([
            {"id": busy, "code": "conversation_busy"},
            {"id": missing, "code": "no_such_conversation"},
        ])
    );
    owner
        .chat_conversation(&busy)
        .await
        .expect("the busy conversation is kept");
    let audit = audit_lines(&host);
    let line = audit
        .iter()
        .find(|line| line["action"] == "conversations_delete")
        .expect("one audit line");
    assert_eq!(line["detail"]["deleted"], "2");
    assert_eq!(line["detail"]["failed"], "2");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_body_is_invalid_body_and_deletes_nothing() {
    let host = TestHost::start("bulk-delete-bad").await;
    let (owner, paired) = host.pair("Desktop mal formado").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let kept = owner.chat_create("Se queda", None).await.expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let too_many: Vec<String> = (0..101)
        .map(|n| format!("cnv_0000000000000000000000{n:03}"))
        .collect();
    let too_many: Vec<&str> = too_many.iter().map(String::as_str).collect();
    let bodies: Vec<(&str, Option<Vec<u8>>)> = vec![
        ("empty body", None),
        ("empty list", bulk_body(&[])),
        ("101 ids", bulk_body(&too_many)),
        ("not an id", bulk_body(&["nope"])),
        ("not json", Some(b"{ids".to_vec())),
        (
            "a stray field",
            Some(br#"{"ids":["cnv_x"],"todo":true}"#.to_vec()),
        ),
    ];
    for (case, body) in bodies {
        let (status, text) = owner.request("POST", BULK, body).await.expect(case);
        assert_eq!(status, 400, "{case}: {text}");
        assert_eq!(parse(&text)["detail"], "invalid_body", "{case}");
    }
    owner
        .chat_conversation(&kept)
        .await
        .expect("nothing was deleted");
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_deletes_in_one_request_spend_one_write() {
    let host = TestHost::start("bulk-delete-fifty").await;
    let (owner, paired) = host.pair("Desktop cincuenta").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    // Creating them one by one through HTTP would spend the budget first: the store does it.
    let ids: Vec<String> = (0..50)
        .map(|_| {
            host.chat
                .create_conversation(NewConversation {
                    title: None,
                    project_id: None,
                })
                .expect("create")
                .id
        })
        .collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();

    let (status, text) = owner
        .request("POST", BULK, bulk_body(&refs))
        .await
        .expect("bulk");
    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["deleted"].as_array().expect("array").len(), 50);

    let (status, text) = owner
        .request(
            "POST",
            "/v1/chat/conversations",
            Some(br#"{"title":"despues"}"#.to_vec()),
        )
        .await
        .expect("a normal write after the batch");
    assert_eq!(status, 200, "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reader_cannot_batch_delete() {
    let host = TestHost::start("bulk-delete-reader").await;
    let (owner, paired) = host.pair("Desktop dueño").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let kept = owner.chat_create("Protegida", None).await.expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let reader = host
        .paired_with("Solo lectura", &[Permission::ReadState])
        .await;

    let (status, text) = reader
        .request("POST", BULK, bulk_body(&[kept.as_str()]))
        .await
        .expect("reader");

    assert_eq!(status, 403, "{text}");
    owner.chat_conversation(&kept).await.expect("still there");
}
