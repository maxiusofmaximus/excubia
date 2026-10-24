//! `POST /v1/workspaces` and the hook channel (`POST /v1/hooks/report`).

mod harness;

use std::time::Duration;

use harness::{row_of, wait_for_status, TestHost};
use hn_core_auth::Permission;
use hn_core_test_support::SignedClient;
use serde_json::json;

async fn create(client: &SignedClient, agent: &str, cwd: &str, label: &str) -> (u16, String) {
    let body =
        serde_json::to_vec(&json!({"agent": agent, "cwd": cwd, "label": label})).expect("json");
    client
        .request("POST", "/v1/workspaces", Some(body))
        .await
        .expect("request")
}

fn workdir() -> String {
    env!("CARGO_MANIFEST_DIR").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_creates_a_conversation_and_hooks_drive_its_state() {
    let host = TestHost::start_with("workspaces", Duration::from_millis(400)).await;
    let client = host.paired_with("crear", &[Permission::Respond]).await;

    // Without RESPOND the endpoint is refused before anything is started.
    let reader = host.paired_with("solo lectura", &[]).await;
    let (status, body) = create(&reader, "fake", &workdir(), "no").await;
    assert_eq!(status, 403, "{body}");

    // The closed catalogue: unknown names and unavailable commands are
    // dedicated 422 codes with an explicit field.
    let (status, body) = create(&client, "rm -rf /", &workdir(), "no").await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"code\":\"unknown_agent\""), "{body}");
    assert!(body.contains("\"field\":\"agent\""), "{body}");
    let (status, body) = create(&client, "missing", &workdir(), "no").await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"code\":\"agent_unavailable\""), "{body}");
    assert!(body.contains("\"field\":\"agent\""), "{body}");
    let (status, body) = create(&client, "fake", "definitely/not/here", "no").await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"code\":\"invalid_cwd\""), "{body}");
    assert!(body.contains("\"field\":\"cwd\""), "{body}");
    let long_label = "a".repeat(65);
    let (status, body) = create(&client, "fake", &workdir(), &long_label).await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"code\":\"invalid_label\""), "{body}");
    assert!(body.contains("\"field\":\"label\""), "{body}");
    // A JSON body of the wrong shape keeps the generic `invalid_body` answer.
    let (status, body) = client
        .request(
            "POST",
            "/v1/workspaces",
            Some(br#"{"agent": 42, "cwd": "."}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"code\":\"invalid_body\""), "{body}");
    assert!(body.contains("\"loc\""), "{body}");

    // A real creation: pane id, initial record, and the label as its title.
    let (status, body) = create(&client, "fake", &workdir(), "mi chat").await;
    assert_eq!(status, 200, "{body}");
    let created: serde_json::Value = serde_json::from_str(&body).expect("json");
    let pane_id = created["pane_id"].as_str().expect("pane_id").to_string();
    assert_eq!(created["workspace_id"], "w1");
    assert_eq!(created["agent"]["agent"], "fake");
    assert_eq!(created["ok"], true);

    let row = row_of(&client, &pane_id).await.expect("row");
    assert_eq!(row["title"], "mi chat");

    // The agent reaches its approval dialog.
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "the agent never blocked"
    );

    // A fresh hook wins over the screen pattern.
    let (status, body) = host
        .report_hook(&pane_id, "working", &host.hook_token)
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"agent_status\":\"working\""), "{body}");
    let row = row_of(&client, &pane_id).await.expect("row");
    assert_eq!(row["status"], "working", "the hook wins while fresh");

    // The hook expires and the screen speaks again.
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(5)).await,
        "the pattern never took over after the hook expired"
    );

    // Hook failures: a wrong token, an unknown pane, an unknown state.
    let (status, _) = host.report_hook(&pane_id, "idle", "not-the-token").await;
    assert_eq!(status, 401);
    let (status, body) = host.report_hook("w1:p99", "idle", &host.hook_token).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("\"code\":\"pane_gone\""), "{body}");
    let (status, body) = host
        .report_hook(&pane_id, "exploded", &host.hook_token)
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("\"loc\":[\"state\"]"), "{body}");

    // A hook may name an unnamed pane, but it never renames one.
    let (status, _) = host.report_hook(&pane_id, "idle", &host.hook_token).await;
    assert_eq!(status, 200);
    let row = row_of(&client, &pane_id).await.expect("row");
    assert_eq!(row["title"], "mi chat", "the label is not overwritten");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_restores_the_panes_with_their_last_screen() {
    let host = TestHost::start("restore").await;
    let client = host.paired_with("restaurar", &[Permission::Respond]).await;

    let (status, body) = create(&client, "fake", &workdir(), "sobrevive").await;
    assert_eq!(status, 200, "{body}");
    let created: serde_json::Value = serde_json::from_str(&body).expect("json");
    let pane_id = created["pane_id"].as_str().expect("pane_id").to_string();
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "the agent never blocked"
    );

    // A restart: the agents are not relaunched, their panes are kept.
    host.host.shutdown().await;
    let host = host.restart(Duration::from_secs(45)).await;
    let client = host
        .paired_with("restaurador", &[Permission::Respond])
        .await;

    let row = row_of(&client, &pane_id).await.expect("restored row");
    assert_eq!(row["restored"], true, "{row}");
    assert_eq!(row["status"], "unknown", "a restored pane knows nothing");
    assert_eq!(row["title"], "sobrevive");

    // The last screen survived, so the client never sees an empty pane.
    let read = client.read(&pane_id, 40).await.expect("read");
    let text = read["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("Esc to cancel · Enter to confirm"),
        "the dialog did not survive: {text}"
    );

    // Nothing can drive a process that is gone.
    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/agents/{pane_id}/respond"),
            Some(br#"{"text":"y"}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("no running agent"), "{body}");
    let (status, body) = host.report_hook(&pane_id, "idle", &host.hook_token).await;
    assert_eq!(status, 409, "{body}");

    // The restored pane is part of the new snapshot, so a second restart
    // keeps it too.
    host.host.shutdown().await;
    let host = host.restart(Duration::from_secs(45)).await;
    let (client, paired) = host.pair("segundo reinicio").await;
    host.grant(&paired.device_id, Permission::ReadState);
    let row = row_of(&client, &pane_id).await.expect("still restored");
    assert_eq!(row["restored"], true);
    let read = client.read(&pane_id, 5).await.expect("read");
    assert!(
        read["text"].as_str().unwrap_or_default().contains("cancel"),
        "the last screen survives a second restart: {read}"
    );
}
