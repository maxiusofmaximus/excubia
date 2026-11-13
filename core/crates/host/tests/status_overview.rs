//! `status.overview` against a real host: panels in three states, a long
//! delegation and an approval waiting at Alfred's level give the counts of the
//! state and of the bootstrap, and only Alfred can read the overview.

mod harness;

use std::path::Path;
use std::time::Duration;

use excubia_chat::{
    action_digest, ApprovalKind, NewApproval, NewConversation, NewDelegation, NewProject, NewSeat,
    SeatActor, DEFAULT_TTL_SECONDS,
};
use excubia_host::paths;
use excubia_state::{NewAgent, Status};
use harness::{SeatKind, TestHost};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

/// Alfred, one project with its lead and a specialist, and a conversation.
struct Company {
    alfred: String,
    lead: String,
    specialist: String,
    project_id: String,
    conversation: String,
}

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

fn seat_actor(
    agent_id: &str,
    role_id: &str,
    project_id: Option<&str>,
    conversation: &str,
) -> SeatActor {
    SeatActor {
        agent_id: agent_id.to_string(),
        role_id: role_id.to_string(),
        project_id: project_id.map(str::to_string),
        conversation_id: conversation.to_string(),
        run_id: String::new(),
    }
}

fn company(host: &TestHost) -> Company {
    let chat = &host.chat;
    let project = chat
        .create_project(NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let specialist = chat
        .create_seat(NewSeat {
            project_id: project.id.clone(),
            name: "Ana".to_string(),
            role_id: "specialist".to_string(),
        })
        .expect("specialist");
    let conversation = chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: Some(project.id.clone()),
        })
        .expect("conversation")
        .id;
    Company {
        alfred: chat.alfred_id().expect("alfred"),
        lead: project.lead.as_ref().expect("lead").agent_id.clone(),
        specialist: specialist.agent_id,
        project_id: project.id,
        conversation,
    }
}

/// The token of Alfred and of the lead, granted as a panel start grants them.
fn grant(host: &TestHost, company: &Company) {
    let team = host.team.as_ref().expect("a team host");
    team.grant_for_tests(
        "tok-alfred",
        seat_actor(&company.alfred, "alfred", None, &company.conversation),
        "pane_alfred",
    );
    team.grant_for_tests(
        "tok-lead",
        seat_actor(
            &company.lead,
            "lead",
            Some(&company.project_id),
            &company.conversation,
        ),
        "pane_lead",
    );
}

/// A panel of the state store, in `status`, with its seat when it has one.
async fn panel(host: &TestHost, pane_id: &str, status: Status, cwd: &Path) {
    host.store
        .add_agent(NewAgent {
            pane_id: pane_id.to_string(),
            workspace_id: "ws_1".to_string(),
            project: "excubia".to_string(),
            agent: "claude".to_string(),
            cwd: cwd.to_string_lossy().to_string(),
            label: None,
            restored: false,
        })
        .await;
    host.store.set_status(pane_id, status).await;
}

/// A command the specialist asks for. It waits at the lead; see `raise_to_cto`.
fn ask(host: &TestHost, company: &Company, command: &str) -> String {
    let args = json!({ "command": command });
    let cwd = host.data_dir.join("demo").to_string_lossy().to_string();
    host.chat
        .create_approval(NewApproval {
            conversation_id: Some(company.conversation.clone()),
            seat_id: Some(company.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval")
        .id
}

/// The escalation of a later phase: the approval waits at Alfred's level.
fn raise_to_cto(host: &TestHost, approval_id: &str) {
    let connection =
        rusqlite::Connection::open(paths::chat_db_path(&host.data_dir)).expect("raw database");
    connection
        .execute(
            "UPDATE approval SET level = 'cto' WHERE id = ?1",
            [approval_id],
        )
        .expect("raise the level");
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

#[tokio::test(flavor = "multi_thread")]
async fn the_overview_counts_what_the_state_and_the_bootstrap_count() {
    let host = start("status-overview-counts").await;
    let company = company(&host);
    grant(&host, &company);
    let chat = &host.chat;
    let demo = host.data_dir.join("demo");

    panel(&host, "pane_lead", Status::Working, &demo).await;
    panel(&host, "pane_ana", Status::Blocked, &demo).await;
    panel(&host, "pane_loose", Status::Idle, &host.data_dir).await;
    chat.set_seat_pane(&company.lead, Some("pane_lead"))
        .expect("lead pane");
    chat.set_seat_pane(&company.specialist, Some("pane_ana"))
        .expect("specialist pane");

    let long_task =
        "Revisa el presupuesto trimestral y compara cada partida con el real ".repeat(3);
    chat.create_delegation(NewDelegation {
        conversation_id: company.conversation.clone(),
        from_agent_id: company.alfred.clone(),
        to_agent_id: company.lead.clone(),
        task: long_task,
        turn_key: "run_status".to_string(),
        parent_id: None,
        steps: 1,
        plan_id: None,
    })
    .expect("delegation");
    let approval = ask(&host, &company, "npm install left-pad");
    raise_to_cto(&host, &approval);

    let (status, body) = call(&host, "tok-alfred", "status.overview", json!({})).await;
    assert_eq!(status, 200, "{body}");
    let overview = &body["result"];

    let state = host.store.payload().await;
    let bootstrap = chat.bootstrap().expect("bootstrap");
    assert_eq!(overview["revision"], json!(state.revision));
    assert_eq!(overview["chat_revision"], json!(bootstrap.chat.revision));
    assert_eq!(overview["panels"]["counts"], json!(state.counts));
    assert_eq!(overview["panels"]["total"], json!(state.agents.len()));
    assert_eq!(
        overview["delegations"]["open"],
        json!(bootstrap.chat.active_delegations)
    );
    assert_eq!(overview["approvals"]["pending"], json!(1));
    assert_eq!(
        overview["approvals"]["pending"],
        json!(bootstrap.chat.pending_approvals)
    );

    let projects = &overview["panels"]["projects"];
    assert_eq!(projects[0]["name"], "Demo");
    assert_eq!(projects[0]["counts"], json!({ "blocked": 1, "working": 1 }));
    assert_eq!(overview["panels"]["unassigned"], json!({ "idle": 1 }));

    let task = overview["delegations"]["items"][0]["task"]
        .as_str()
        .expect("task");
    assert_eq!(task.chars().count(), 120, "{task}");
    assert!(task.ends_with('…'), "{task}");
    assert_eq!(overview["approvals"]["items"][0]["agent"], "ana");
    assert_eq!(overview["approvals"]["items"][0]["kind"], "command");

    let text = body.to_string();
    let data_dir = host.data_dir.to_string_lossy().replace('\\', "\\\\");
    for leaked in [
        "npm install left-pad",
        "pane_",
        "root_path",
        "hook_token",
        "payload_redacted",
        "\"cwd\"",
        data_dir.as_str(),
    ] {
        assert!(!text.contains(leaked), "{leaked} in {text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_company_answers_a_valid_empty_overview() {
    let host = start("status-overview-empty").await;
    let alfred = host.chat.alfred_id().expect("alfred");
    host.team.as_ref().expect("a team host").grant_for_tests(
        "tok-alfred",
        seat_actor(&alfred, "alfred", None, ""),
        "pane_alfred",
    );

    let (status, body) = call(&host, "tok-alfred", "status.overview", json!({})).await;
    assert_eq!(status, 200, "{body}");
    let overview = &body["result"];
    assert_eq!(overview["panels"]["total"], 0);
    assert_eq!(overview["panels"]["counts"], json!({}));
    assert_eq!(overview["panels"]["projects"], json!([]));
    assert_eq!(overview["delegations"]["items"], json!([]));
    assert_eq!(overview["approvals"]["pending"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_alfred_reads_it_and_it_takes_no_arguments() {
    let host = start("status-overview-scope").await;
    let company = company(&host);
    grant(&host, &company);

    let (status, body) = call(&host, "tok-lead", "status.overview", json!({})).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "scope_denied", "{body}");

    let (status, body) = call(
        &host,
        "tok-alfred",
        "status.overview",
        json!({ "project_id": company.project_id }),
    )
    .await;
    assert_eq!(status, 422, "{body}");
}
