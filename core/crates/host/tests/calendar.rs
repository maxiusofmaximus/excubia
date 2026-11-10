//! The calendar of Alfred against a real host (CALENDARIO-F4): an event lands in the
//! company's zone and is listed back; with no zone Alfred keeps nothing and says so; a
//! lead cannot touch the calendar; and a delete waits for the user's approval, so the
//! event stays until the user approves it and survives a refusal.

mod harness;

use std::time::Duration;

use harness::{SeatKind, TestHost};
use excubia_core_chat::{
    Approval, ApprovalFilter, ApprovalStatus, ChatStore, Decision, NewConversation, SeatActor,
};
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
        .header("X-Excubia-Tool-Token", token)
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

fn pending(chat: &ChatStore) -> Vec<Approval> {
    chat.approvals(&ApprovalFilter {
        status: Some(ApprovalStatus::Pending),
        conversation_id: None,
        limit: 20,
    })
    .expect("approvals")
}

/// The one delete question on the device, as the device lists it.
fn pending_delete(chat: &ChatStore) -> Approval {
    let mut questions = pending(chat);
    assert_eq!(questions.len(), 1, "one question per event");
    let question = questions.remove(0);
    assert_eq!(question.tool, "calendar.delete");
    assert_eq!(
        question.level.as_deref(),
        Some("user"),
        "the lead never decides it"
    );
    question
}

async fn create_event(host: &TestHost, token: &str, title: &str) -> String {
    let (status, body) = call(
        host,
        token,
        "calendar.create",
        json!({ "title": title, "start": "2026-11-03T09:30" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body["result"]["id"].as_str().expect("event id").to_string()
}

fn listed_ids(body: &Value) -> Vec<String> {
    body["result"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["id"].as_str().expect("id").to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn alfred_keeps_an_event_in_the_company_zone_and_lists_it_back() {
    let host = start("calendar-keep").await;
    let chat = &host.chat;
    chat.set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);

    let id = create_event(&host, "tok-alfred", "Llamada con Ana").await;
    assert_eq!(chat.event(&id).expect("event").timezone, "Europe/Madrid");

    let (status, body) = call(
        &host,
        "tok-alfred",
        "calendar.list",
        json!({ "from": "2026-11-01", "to": "2026-11-08" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(listed_ids(&body), vec![id.clone()]);

    let (status, body) = call(&host, "tok-alfred", "calendar.timezone", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["timezone"], "Europe/Madrid");
}

/// Alfred keeps a series; a list shows each occurrence with its own start.
#[tokio::test(flavor = "multi_thread")]
async fn alfred_keeps_a_series_and_lists_its_occurrences() {
    let host = start("calendar-series").await;
    let chat = &host.chat;
    chat.set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);

    let (status, body) = call(
        &host,
        "tok-alfred",
        "calendar.create",
        json!({ "title": "Clase", "start": "2026-11-03T09:30", "recurrence": "FREQ=DAILY;COUNT=2" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["result"]["recurrence"],
        "FREQ=DAILY;INTERVAL=1;COUNT=2"
    );

    let (status, body) = call(
        &host,
        "tok-alfred",
        "calendar.list",
        json!({ "from": "2026-11-01", "to": "2026-11-08" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let starts: Vec<&str> = body["result"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["occurrence_start"].as_str().expect("start"))
        .collect();
    assert_eq!(
        starts,
        ["2026-11-03T08:30:00.000Z", "2026-11-04T08:30:00.000Z"]
    );
    assert_eq!(body["result"]["truncated"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_zone_alfred_keeps_nothing_and_says_so() {
    let host = start("calendar-zone-unset").await;
    let chat = &host.chat;
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);

    let (status, body) = call(
        &host,
        "tok-alfred",
        "calendar.create",
        json!({ "title": "Dentista", "start": "2026-11-04T10:00" }),
    )
    .await;
    assert_ne!(status, 200, "{body}");
    assert_eq!(body["detail"]["code"], "timezone_unset");
    assert!(
        pending(chat).is_empty(),
        "no question for an event never kept"
    );

    let (status, body) = call(&host, "tok-alfred", "calendar.timezone", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert!(body["result"]["timezone"].is_null(), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lead_cannot_touch_the_calendar() {
    let host = start("calendar-scope").await;
    let chat = &host.chat;
    chat.set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);
    let id = create_event(&host, "tok-alfred", "De Alfred").await;
    let project = chat
        .create_project(excubia_core_chat::NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let lead = project.lead.as_ref().expect("lead").agent_id.clone();
    grant(&host, "tok-lead", "lead", &lead, &conversation_id);

    for (tool, arguments) in [
        (
            "calendar.list",
            json!({ "from": "2026-11-01", "to": "2026-11-08" }),
        ),
        (
            "calendar.create",
            json!({ "title": "Del lead", "start": "2026-11-05T09:00" }),
        ),
        ("calendar.delete", json!({ "id": id })),
    ] {
        let (status, body) = call(&host, "tok-lead", tool, arguments).await;
        assert_ne!(status, 200, "{tool}");
        assert_eq!(body["detail"]["code"], "scope_denied", "{tool}");
    }
    assert!(
        chat.event(&id).is_ok(),
        "the lead's refused delete keeps the event"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delete_waits_for_the_user_and_the_event_goes_only_when_approved() {
    let host = start("calendar-delete-approve").await;
    let chat = &host.chat;
    chat.set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);
    let id = create_event(&host, "tok-alfred", "Revisión").await;

    let (status, body) = call(&host, "tok-alfred", "calendar.delete", json!({ "id": id })).await;
    assert_eq!(status, 200, "{body}");
    let question = pending_delete(chat);
    assert_eq!(body["result"]["reason"], "pending_approval", "{body}");
    assert!(chat.event(&id).is_ok(), "the event waits for the answer");

    chat.decide_approval(&question.id, Decision::Approve, "device-1", "device-1")
        .expect("the device decides");
    assert!(chat.event(&id).is_err(), "approved, the event is gone");
    assert_eq!(
        chat.approval(&question.id).expect("approval").status,
        ApprovalStatus::Consumed
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_delete_keeps_the_event() {
    let host = start("calendar-delete-deny").await;
    let chat = &host.chat;
    chat.set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let conversation_id = conversation(chat);
    let alfred = chat.alfred_id().expect("alfred");
    grant(&host, "tok-alfred", "alfred", &alfred, &conversation_id);
    let id = create_event(&host, "tok-alfred", "Revisión").await;

    let (status, body) = call(&host, "tok-alfred", "calendar.delete", json!({ "id": id })).await;
    assert_eq!(status, 200, "{body}");
    let question = pending_delete(chat);
    chat.decide_approval(&question.id, Decision::Deny, "device-1", "device-1")
        .expect("the device refuses");
    assert_eq!(chat.event(&id).expect("kept").title, "Revisión");
}
