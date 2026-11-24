//! A CLI with no resident panel mode is refused by the two entry points that create a
//! seat (`project.create`, `agent.spawn`) before anything is written: no project row, no
//! folder, no seat row and no UAC canary. The refusal carries a stable code.

mod harness;

use std::path::PathBuf;
use std::time::Duration;

use excubia_chat::{NewConversation, SeatActor};
use harness::{SeatKind, TestHost};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

async fn start(name: &str) -> TestHost {
    TestHost::start_with_seat(
        name,
        TTL,
        SeatKind::TeamWithoutResident {
            turn_timeout: CLI_TIMEOUT,
            delegation_timeout: CLI_TIMEOUT,
            // A plugin is configured, so a canary would run if the seat got that far.
            uac_plugin: Some(PathBuf::from("tooling")),
        },
    )
    .await
}

fn seat_actor(agent_id: &str, role_id: &str, conversation: &str) -> SeatActor {
    SeatActor {
        agent_id: agent_id.to_string(),
        role_id: role_id.to_string(),
        project_id: None,
        conversation_id: conversation.to_string(),
        run_id: String::new(),
    }
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

/// Nothing a seat creation would leave behind exists: no project, no folder, no canary.
fn assert_no_effects(host: &TestHost) {
    let projects = host.chat.bootstrap().expect("bootstrap").projects;
    assert!(projects.is_empty(), "{projects:?}");
    let workspaces = host.data_dir.join("workspaces");
    let folders = std::fs::read_dir(&workspaces)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(folders, 0, "{}", workspaces.display());
    assert!(!host.data_dir.join("uac-canary").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn project_create_is_refused_before_any_row_folder_or_canary() {
    let host = start("resident-refused-project").await;
    let alfred = host.chat.alfred_id().expect("alfred");
    let conversation = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Proyectos".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    host.team.as_ref().expect("a team host").grant_for_tests(
        "tok-alfred",
        seat_actor(&alfred, "alfred", &conversation),
        "pane_alfred",
    );

    let (status, body) = call(
        &host,
        "tok-alfred",
        "project.create",
        json!({ "name": "Proyecto web" }),
    )
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["detail"]["code"], "resident_unsupported", "{body}");
    assert_no_effects(&host);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_spawn_is_refused_before_its_row_exists() {
    let host = start("resident-refused-spawn").await;
    let alfred = host.chat.alfred_id().expect("alfred");
    let conversation = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Equipo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    host.team.as_ref().expect("a team host").grant_for_tests(
        "tok-alfred",
        seat_actor(&alfred, "alfred", &conversation),
        "pane_alfred",
    );

    // The project does not exist: had the gate come after the row, the answer would be
    // `unknown_project` and a seat row would have been tried.
    let (status, body) = call(
        &host,
        "tok-alfred",
        "agent.spawn",
        json!({ "name": "Ernesto", "role": "lead", "project_id": "prj_inexistente" }),
    )
    .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["detail"]["code"], "resident_unsupported", "{body}");
    assert_no_effects(&host);
}
