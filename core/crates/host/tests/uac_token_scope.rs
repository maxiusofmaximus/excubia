//! The hook credential of CORE8B: one token per run, checked against the
//! approval it names.
//!
//! The four refusals are different on the wire because the hook must tell them
//! apart in its log, and because a token that belongs to another run is not the
//! same event as a bad signature. None of them changes the approval.

mod harness;

use std::time::Duration;

use harness::{approval_body, detail, SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_test_support::SignedClient;
use serde_json::Value;

const TTL: Duration = Duration::from_secs(45);

async fn approver_host(name: &str) -> (TestHost, SignedClient) {
    let host = TestHost::start_with_seat(name, TTL, SeatKind::Fake).await;
    let (client, paired) = host.pair("Desktop uac").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    (host, client)
}

fn approval_id(body: &str) -> String {
    serde_json::from_str::<Value>(body).expect("json")["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string()
}

async fn pending(client: &SignedClient) -> Vec<Value> {
    let inbox = client
        .chat_approvals(&[("status", "pending")])
        .await
        .expect("inbox");
    inbox["approvals"].as_array().expect("approvals").clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_token_of_another_run_is_refused_and_changes_nothing() {
    let (host, client) = approver_host("scope-foreign").await;
    let owner = host.run_hook("scope-owner");
    let intruder = host.run_hook("scope-intruder");
    assert_ne!(owner.run_id, intruder.run_id);

    let (status, body) = host
        .hook_request(
            &owner.token,
            "POST",
            "/v1/chat/approvals",
            Some(approval_body("cargo test")),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = approval_id(&body);

    // The approval belongs to the owner's run: the other run cannot even read
    // its state, and it cannot burn it.
    let (status, body) = host
        .hook_request(
            &intruder.token,
            "GET",
            &format!("/v1/chat/approvals/{approval_id}"),
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(detail(&body), "hook_scope_mismatch");

    let (status, body) = host
        .hook_request(
            &intruder.token,
            "POST",
            &format!("/v1/chat/approvals/{approval_id}/consume"),
            Some(Vec::new()),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(detail(&body), "hook_scope_mismatch");

    // No state changed: it is still pending and still nobody's but the owner's.
    let inbox = pending(&client).await;
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0]["id"], approval_id.as_str());
    assert_eq!(inbox[0]["run_id"], owner.run_id.as_str());
    assert_eq!(
        inbox[0]["conversation_id"],
        owner.conversation_id.as_str(),
        "the token stamped the run and its conversation"
    );

    // The owner reads it, which proves the refusal was the scope, not the id.
    let (status, body) = host
        .hook_request(
            &owner.token,
            "GET",
            &format!("/v1/chat/approvals/{approval_id}"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let state: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(state["status"], "pending");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_token_is_forbidden() {
    let (host, _client) = approver_host("scope-revoked").await;
    let hook = host.run_hook("scope-revoked");
    assert!(host.chat.revoke_hook_token(&hook.run_id).expect("revoke"));

    // A closed run does not get a 401: its token is known and revoked.
    let (status, body) = host
        .hook_request(
            &hook.token,
            "POST",
            "/v1/chat/approvals",
            Some(approval_body("cargo test")),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(detail(&body), "hook_token_revoked");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_token_cannot_open_an_approval() {
    let (host, _client) = approver_host("scope-host").await;
    let (status, body) = host
        .hook_request(
            &host.hook_token,
            "POST",
            "/v1/chat/approvals",
            Some(approval_body("cargo test")),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(detail(&body), "hook_token_not_scoped");

    // It still reports pane state: that is what the host token is for.
    let pane_id = host.spawn_fake(&[]).await;
    let (status, body) = host.report_hook(&pane_id, "idle", &host.hook_token).await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_digest_is_never_reopened() {
    let (host, client) = approver_host("scope-digest").await;
    let hook = host.run_hook("scope-digest");
    let (status, body) = host
        .hook_request(
            &hook.token,
            "POST",
            "/v1/chat/approvals",
            Some(approval_body("cargo test")),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = approval_id(&body);

    let denied = client
        .chat_decide_approval(&approval_id, "deny", "device-1")
        .await
        .expect("decision");
    assert_eq!(denied["approval"]["status"], "denied");

    // The same action, again: the digest is the identity of the action, so a
    // denial does not make it available.
    let (status, body) = host
        .hook_request(
            &hook.token,
            "POST",
            "/v1/chat/approvals",
            Some(approval_body("cargo test")),
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(detail(&body), "approval_digest_reused");

    // Nothing was reopened: one resolved row, nothing pending.
    assert!(pending(&client).await.is_empty());
    let denied_rows = client
        .chat_approvals(&[("status", "denied")])
        .await
        .expect("inbox");
    let rows = denied_rows["approvals"].as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], approval_id.as_str());
    assert_eq!(rows[0]["status"], "denied");
}
