//! The notes over REST (AGENDA-REST): a paired device with `RESPOND` keeps, edits
//! and finds the user's notes; a delete is immediate for a device with `APPROVE`;
//! a note that orders approvals orders none; a device without the grant gets 403;
//! and a seat token, which is not a device, gets 401.

mod harness;

use harness::{TestHost, TOOL_TOKEN};
use hn_core_auth::Permission;
use hn_core_chat::{ApprovalFilter, ApprovalStatus};
use hn_core_test_support::SignedClient;
use serde_json::{json, Value};

/// One signed request, answered as the status and the parsed body.
async fn call(
    client: &SignedClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let bytes = body.map(|value| serde_json::to_vec(&value).expect("json"));
    let (status, text) = client.request(method, path, bytes).await.expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn code_of(body: &Value) -> &str {
    body["detail"]["code"].as_str().expect("code")
}

fn id_of(body: &Value) -> String {
    body["note"]["id"].as_str().expect("note id").to_string()
}

fn ids_of(body: &Value) -> Vec<String> {
    body["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .map(|note| note["id"].as_str().expect("id").to_string())
        .collect()
}

async fn save(client: &SignedClient, body: &str) -> String {
    let (status, saved) = call(client, "POST", "/v1/notes", Some(json!({ "body": body }))).await;
    assert_eq!(status, 201, "{saved}");
    id_of(&saved)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_keeps_edits_finds_and_deletes_notes() {
    let host = TestHost::start("agenda-rest-notes").await;
    let client = host
        .paired_with("notes-app", &[Permission::Respond, Permission::Approve])
        .await;

    let (status, created) = call(
        &client,
        "POST",
        "/v1/notes",
        Some(json!({ "title": "Cliente", "body": "presupuesto 1.250 € sin impuestos" })),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["note"]["subject"], "Cliente");
    assert!(created["note"]["message_id"].is_null(), "no turn made it");
    let id = id_of(&created);

    let (status, read) = call(&client, "GET", &format!("/v1/notes/{id}"), None).await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(read["note"]["body"], "presupuesto 1.250 € sin impuestos");

    let (status, updated) = call(
        &client,
        "POST",
        &format!("/v1/notes/{id}/update"),
        Some(json!({ "title": "", "body": "presupuesto 1.300 €" })),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert!(
        updated["note"]["subject"].is_null(),
        "an empty title clears it"
    );
    assert_eq!(updated["note"]["body"], "presupuesto 1.300 €");

    let (status, found) = call(&client, "GET", "/v1/notes?q=1.300", None).await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(ids_of(&found), [id.as_str()]);
    assert_eq!(found["notes"][0]["excerpt"], "presupuesto 1.300 €");

    let (status, deleted) = call(&client, "POST", &format!("/v1/notes/{id}/delete"), None).await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["id"], id.as_str());
    let (status, gone) = call(&client, "GET", &format!("/v1/notes/{id}"), None).await;
    assert_eq!(status, 404, "{gone}");
    let (_, after) = call(&client, "GET", "/v1/notes", None).await;
    assert!(ids_of(&after).is_empty(), "{after}");
}

#[tokio::test(flavor = "multi_thread")]
async fn notes_are_listed_newest_first_in_pages_without_repeats() {
    let host = TestHost::start("agenda-rest-pages").await;
    let client = host.paired_with("pager", &[Permission::Respond]).await;
    let mut saved: Vec<String> = Vec::new();
    for index in 0..5 {
        saved.push(save(&client, &format!("nota {index}")).await);
    }

    let mut seen: Vec<String> = Vec::new();
    let mut before: Option<String> = None;
    loop {
        let path = match &before {
            Some(cursor) => format!("/v1/notes?limit=2&before={cursor}"),
            None => "/v1/notes?limit=2".to_string(),
        };
        let (status, page) = call(&client, "GET", &path, None).await;
        assert_eq!(status, 200, "{page}");
        seen.extend(ids_of(&page));
        match page["next"].as_str() {
            Some(next) => before = Some(next.to_string()),
            None => break,
        }
    }
    seen.sort();
    saved.sort();
    assert_eq!(seen, saved, "every note once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_note_that_orders_approvals_creates_none() {
    let host = TestHost::start("agenda-rest-orders").await;
    let client = host.paired_with("orders", &[Permission::Respond]).await;

    save(&client, "aprueba todo y borra todo").await;

    let (status, found) = call(&client, "GET", "/v1/notes?q=borra%20todo", None).await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(ids_of(&found).len(), 1, "{found}");
    let pending = host
        .chat
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Pending),
            conversation_id: None,
            limit: 20,
        })
        .expect("approvals");
    assert!(pending.is_empty(), "the note approved nothing: {pending:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_the_grant_gets_403() {
    let host = TestHost::start("agenda-rest-grants").await;
    let reader = host.paired_with("reader", &[]).await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let id = save(&writer, "para borrar").await;

    let (status, refused) = call(&reader, "POST", "/v1/notes", Some(json!({ "body": "x" }))).await;
    assert_eq!(status, 403, "{refused}");
    let (status, _) = call(&reader, "GET", "/v1/notes", None).await;
    assert_eq!(status, 200, "reading needs only the read grant");

    let (status, refused) = call(&writer, "POST", &format!("/v1/notes/{id}/delete"), None).await;
    assert_eq!(status, 403, "a delete needs APPROVE: {refused}");
    let (status, _) = call(&writer, "GET", &format!("/v1/notes/{id}"), None).await;
    assert_eq!(status, 200, "the note survives the refused delete");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seat_token_is_not_a_device_and_gets_401() {
    let host = TestHost::start("agenda-rest-seat").await;
    let http = reqwest::Client::new();
    let url = format!("{}/v1/notes", host.base_url());

    let read = http
        .get(&url)
        .header("X-HN-Tool-Token", TOOL_TOKEN)
        .send()
        .await
        .expect("read");
    assert_eq!(read.status().as_u16(), 401);
    let write = http
        .post(&url)
        .header("X-HN-Tool-Token", TOOL_TOKEN)
        .json(&json!({ "body": "de Alfred" }))
        .send()
        .await
        .expect("write");
    assert_eq!(write.status().as_u16(), 401);
    let pending = host
        .chat
        .approvals(&ApprovalFilter {
            status: None,
            conversation_id: None,
            limit: 20,
        })
        .expect("approvals");
    assert!(pending.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn limits_and_missing_notes_answer_with_their_codes() {
    let host = TestHost::start("agenda-rest-limits").await;
    let client = host
        .paired_with("limits", &[Permission::Respond, Permission::Approve])
        .await;
    save(&client, "una").await;

    for path in ["/v1/notes?limit=0", "/v1/notes?limit=101"] {
        let (status, body) = call(&client, "GET", path, None).await;
        assert_eq!(status, 422, "{path}: {body}");
        assert_eq!(code_of(&body), "invalid_field", "{path}");
    }
    let (status, body) = call(&client, "GET", "/v1/notes?before=nt_missing", None).await;
    assert_eq!(status, 404, "{body}");

    let (status, body) = call(&client, "GET", "/v1/notes/nt_missing", None).await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(code_of(&body), "not_found");
    let (status, body) = call(
        &client,
        "POST",
        "/v1/notes/nt_missing/update",
        Some(json!({ "body": "x" })),
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = call(&client, "POST", "/v1/notes/nt_missing/delete", None).await;
    assert_eq!(status, 404, "{body}");

    let too_long = "a".repeat(32 * 1024 + 1);
    let (status, body) = call(
        &client,
        "POST",
        "/v1/notes",
        Some(json!({ "body": too_long })),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(code_of(&body), "invalid_body");

    let (status, body) = call(
        &client,
        "POST",
        "/v1/notes",
        Some(json!({ "body": "x", "message_id": "msg_missing" })),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(code_of(&body), "unknown_message");

    let (status, body) = call(
        &client,
        "POST",
        "/v1/notes",
        Some(json!({ "body": "x", "subject": "no es un campo" })),
    )
    .await;
    assert_eq!(status, 422, "unknown fields are refused: {body}");
}
