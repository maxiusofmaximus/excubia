//! A pane change against a real host (F5): in `all` mode a blocked agent alerts by default, a
//! `notify` rule adds its id to the live frame, and a `mute` rule silences the change; in
//! `rules_only` mode only a `notify` rule alerts. The live frame carries no agent name.

mod harness;

use std::time::Duration;

use harness::TestHost;
use excubia_core_auth::Permission;
use excubia_core_state::{NewAgent, SharedFrame, Status};
use excubia_core_test_support::SignedClient;
use serde_json::{json, Value};
use tokio::sync::broadcast::{error::TryRecvError, Receiver};

/// The `alert` frames received since the last call.
fn alerts(receiver: &mut Receiver<SharedFrame>) -> Vec<Value> {
    let mut found = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(frame) if frame.event == "alert" => found.push(frame.body.clone()),
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Lagged(_)) => return found,
            Err(TryRecvError::Closed) => return found,
        }
    }
}

/// Lets the judge's task run until `count` frames have arrived, then returns them. A test
/// that expects silence sleeps first and checks with `alerts`.
async fn alerts_within(receiver: &mut Receiver<SharedFrame>, count: usize) -> Vec<Value> {
    let mut found = Vec::new();
    for _ in 0..200 {
        found.extend(alerts(receiver));
        if found.len() >= count {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    found
}

fn agent(pane_id: &str) -> NewAgent {
    NewAgent {
        pane_id: pane_id.to_string(),
        workspace_id: "ws-1".to_string(),
        project: "test".to_string(),
        agent: "fake".to_string(),
        cwd: "/work".to_string(),
        label: None,
        restored: false,
    }
}

async fn set_mode(client: &SignedClient, mode: &str) {
    let (status, body) = client
        .request(
            "POST",
            "/v1/alerts/mode",
            Some(serde_json::to_vec(&json!({ "mode": mode })).expect("json")),
        )
        .await
        .expect("request");
    assert_eq!(status, 200, "{body}");
}

async fn create_rule(client: &SignedClient, rule: Value) -> String {
    let (status, body) = client
        .request(
            "POST",
            "/v1/alerts/rules",
            Some(serde_json::to_vec(&rule).expect("json")),
        )
        .await
        .expect("request");
    assert_eq!(status, 201, "{body}");
    let created: Value = serde_json::from_str(&body).expect("json");
    created["rule"]["id"].as_str().expect("rule id").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mode_and_the_rules_decide_which_pane_change_alerts() {
    let host = TestHost::start("alerts-live").await;
    let phone = host.paired_with("phone", &[Permission::Respond]).await;
    let mut receiver = host.store.receiver();

    // `all` by default: a blocked agent alerts with no rule, and the frame names none.
    host.store.add_agent(agent("pane-1")).await;
    host.store.set_status("pane-1", Status::Blocked).await;
    let frames = alerts_within(&mut receiver, 1).await;
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["kind"], "agent_blocked");
    assert!(frames[0]["rule_id"].is_null());
    assert_eq!(frames[0].as_object().map(|object| object.len()), Some(3));

    // A mute rule silences the next blocked change, and no frame follows.
    let mute = create_rule(
        &phone,
        json!({ "trigger": "pane_status", "status": "blocked", "action": "mute" }),
    )
    .await;
    host.store.add_agent(agent("pane-2")).await;
    host.store.set_status("pane-2", Status::Blocked).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        alerts(&mut receiver).is_empty(),
        "a muted change sends no frame"
    );

    // `rules_only` with the mute rule removed: still silent without a notify rule.
    let (status, _) = phone
        .request("POST", &format!("/v1/alerts/rules/{mute}/delete"), None)
        .await
        .expect("request");
    assert_eq!(status, 200);
    set_mode(&phone, "rules_only").await;
    host.store.add_agent(agent("pane-3")).await;
    host.store.set_status("pane-3", Status::Blocked).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        alerts(&mut receiver).is_empty(),
        "rules_only without a rule is silent"
    );

    // A notify rule for blocked alerts in `rules_only`, and its id rides on the frame.
    let notify = create_rule(
        &phone,
        json!({ "trigger": "pane_status", "status": "blocked", "action": "notify" }),
    )
    .await;
    host.store.add_agent(agent("pane-4")).await;
    host.store.set_status("pane-4", Status::Blocked).await;
    let frames = alerts_within(&mut receiver, 1).await;
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["rule_id"], notify);

    // A done change has no rule in `rules_only`: no frame.
    host.store.set_status("pane-4", Status::Done).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(alerts(&mut receiver).is_empty());
}
