//! Alfred's alert tools against a real host (F5): Alfred keeps the rules and the mode through
//! the MCP surface, an identical rule is refused, and a lead is refused with `scope_denied`
//! and changes nothing.

mod harness;

use std::time::Duration;

use harness::{SeatKind, TestHost};
use hn_core_chat::{ChatStore, NewConversation, SeatActor};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

async fn start(name: &str) -> TestHost {
    TestHost::start_with_seat(
        name,
        TTL,
        SeatKind::Team {
            turn_timeout: CLI_TIMEOUT,
            delegation_timeout: CLI_TIMEOUT,
            uac_plugin: None,
        },
    )
    .await
}

/// One MCP call with a seat token: the status and the JSON body.
async fn call(host: &TestHost, token: &str, tool: &str, arguments: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", token)
        .json(&json!({ "tool": tool, "arguments": arguments }))
        .send()
        .await
        .expect("tool call");
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("json");
    (status, body)
}

/// Grants `token` the seat `agent_id` in `conversation_id`, as a panel start does.
fn grant(host: &TestHost, token: &str, role_id: &str, agent_id: &str, conversation_id: &str) {
    host.team.as_ref().expect("a team host").grant_for_tests(
        token,
        SeatActor {
            agent_id: agent_id.to_string(),
            role_id: role_id.to_string(),
            project_id: None,
            conversation_id: conversation_id.to_string(),
            run_id: String::new(),
        },
        "pane_test",
    );
}

fn conversation(chat: &ChatStore) -> String {
    chat.create_conversation(NewConversation {
        title: Some("Trabajo".to_string()),
        project_id: None,
    })
    .expect("conversation")
    .id
}

fn reminder_rule() -> Value {
    json!({ "trigger": "calendar_reminder", "action": "notify" })
}

#[tokio::test(flavor = "multi_thread")]
async fn alfred_keeps_rules_and_the_mode_through_the_tools() {
    let host = start("alerts-tools").await;
    let chat = &host.chat;
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);

    let (status, created) = call(&host, "tok-alfred", "alert.rule_create", reminder_rule()).await;
    assert_eq!(status, 200, "{created}");
    let id = created["result"]["rule"]["id"]
        .as_str()
        .expect("rule id")
        .to_string();
    assert_eq!(created["result"]["rule"]["created_by"], "alfred");

    let (status, again) = call(&host, "tok-alfred", "alert.rule_create", reminder_rule()).await;
    assert_ne!(status, 200, "{again}");
    assert_eq!(again["detail"]["code"], "rule_exists");

    let (status, refused) = call(
        &host,
        "tok-alfred",
        "alert.rule_create",
        json!({ "trigger": "pane_status", "action": "notify" }),
    )
    .await;
    assert_ne!(status, 200, "{refused}");
    assert_eq!(refused["detail"]["code"], "invalid_arguments");

    let (status, set) = call(
        &host,
        "tok-alfred",
        "alert.mode_set",
        json!({ "mode": "rules_only" }),
    )
    .await;
    assert_eq!(status, 200, "{set}");
    assert_eq!(set["result"]["mode"], "rules_only");
    assert_eq!(chat.alert_mode().expect("mode").as_str(), "rules_only");

    let (status, listed) = call(&host, "tok-alfred", "alert.rule_list", json!({})).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["result"]["mode"], "rules_only");
    assert_eq!(listed["result"]["rules"].as_array().map(Vec::len), Some(1));

    let (status, updated) = call(
        &host,
        "tok-alfred",
        "alert.rule_update",
        json!({ "id": id, "enabled": false }),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["result"]["rule"]["enabled"], false);

    let (status, deleted) = call(
        &host,
        "tok-alfred",
        "alert.rule_delete",
        json!({ "id": id }),
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    assert!(chat.alert_rules().expect("rules").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lead_cannot_touch_the_alert_rules() {
    let host = start("alerts-tools-scope").await;
    let chat = &host.chat;
    let conversation_id = conversation(chat);
    let project = chat
        .create_project(hn_core_chat::NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let lead = project.lead.as_ref().expect("lead").agent_id.clone();
    grant(&host, "tok-lead", "lead", &lead, &conversation_id);
    chat.create_alert_rule(
        hn_core_chat::AlertWriter::Device("dev_harness"),
        hn_core_chat::NewAlertRule::new("calendar_reminder", None, None, "notify").expect("rule"),
    )
    .expect("a rule the lead must not see");

    for (tool, arguments) in [
        ("alert.rule_create", reminder_rule()),
        ("alert.rule_list", json!({})),
        ("alert.mode_set", json!({ "mode": "rules_only" })),
    ] {
        let (status, body) = call(&host, "tok-lead", tool, arguments).await;
        assert_ne!(status, 200, "{tool}");
        assert_eq!(body["detail"]["code"], "scope_denied", "{tool}");
    }
    assert_eq!(chat.alert_rules().expect("rules").len(), 1);
    assert_eq!(chat.alert_mode().expect("mode").as_str(), "all");
}
