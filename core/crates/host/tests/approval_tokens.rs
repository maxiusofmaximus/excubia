//! The approval tools of the team surface, called with the token of a real
//! seat (CORE9-C): what a lead decides and lists, what the scope refuses with
//! its own 403, and that a host dropped by a test leaves no agent running.

mod harness;

use std::time::{Duration, Instant};

use harness::{process_exists, SeatKind, TestHost};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalStatus, NewApproval, NewConversation,
    NewProject, NewSeat, ProjectSummary, SeatActor, DEFAULT_TTL_SECONDS,
};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

/// Two projects, each with its lead; a specialist in the first one.
struct Office {
    conversation: String,
    alfred: String,
    lead: SeatActor,
    other_lead: SeatActor,
    specialist: SeatActor,
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

fn office(host: &TestHost) -> Office {
    let chat = &host.chat;
    let alfred = chat.alfred_id().expect("alfred");
    let project = chat
        .create_project(NewProject {
            name: "Demo".to_string(),
            root_path: host.data_dir.join("demo"),
        })
        .expect("project");
    let other = chat
        .create_project(NewProject {
            name: "Otro".to_string(),
            root_path: host.data_dir.join("otro"),
        })
        .expect("other project");
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
    let lead_of = |project: &ProjectSummary| project.lead.as_ref().expect("lead").agent_id.clone();
    Office {
        lead: seat_actor(&lead_of(&project), "lead", Some(&project.id), &conversation),
        other_lead: seat_actor(&lead_of(&other), "lead", Some(&other.id), &conversation),
        specialist: seat_actor(
            &specialist.agent_id,
            "specialist",
            Some(&project.id),
            &conversation,
        ),
        conversation,
        alfred,
    }
}

/// The token of each seat, granted as a panel start grants it.
fn grant_all(host: &TestHost, office: &Office) {
    let team = host.team.as_ref().expect("a team host");
    team.grant_for_tests("tok-lead", office.lead.clone(), "pane_lead");
    team.grant_for_tests("tok-other-lead", office.other_lead.clone(), "pane_other");
    team.grant_for_tests("tok-specialist", office.specialist.clone(), "pane_ana");
    team.grant_for_tests(
        "tok-alfred",
        seat_actor(&office.alfred, "alfred", None, &office.conversation),
        "pane_alfred",
    );
}

/// A command the specialist asks for: no rule covers it, so it waits at the lead.
fn ask(host: &TestHost, office: &Office, command: &str) -> Approval {
    let args = json!({ "command": command });
    let cwd = host.data_dir.join("demo").to_string_lossy().to_string();
    host.chat
        .create_approval(NewApproval {
            conversation_id: Some(office.conversation.clone()),
            seat_id: Some(office.specialist.agent_id.clone()),
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

fn listed_ids(body: &Value) -> Vec<String> {
    body["result"]["approvals"]
        .as_array()
        .expect("approvals")
        .iter()
        .map(|approval| approval["id"].as_str().expect("id").to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lead_decides_its_project_with_its_own_token() {
    let host = TestHost::start_with_seat(
        "approval-lead",
        TTL,
        SeatKind::Team {
            turn_timeout: CLI_TIMEOUT,
            delegation_timeout: CLI_TIMEOUT,
            uac_plugin: None,
        },
    )
    .await;
    let office = office(&host);
    grant_all(&host, &office);
    let approval = ask(&host, &office, "npm install left-pad");

    let (status, body) = call(
        &host,
        "tok-lead",
        "approval.decide",
        json!({
            "approval_id": approval.id,
            "decision": "approve",
            "reason": "lo pidió Ana"
        }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], true);

    let decided = host.chat.approval(&approval.id).expect("approval");
    assert_eq!(decided.status, ApprovalStatus::Approved);
    assert_eq!(decided.decider_kind.as_deref(), Some("agent"));
    assert_eq!(
        decided.decider_id.as_deref(),
        Some(office.lead.agent_id.as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scope_the_seat_does_not_hold_is_a_403_with_its_code() {
    let host = TestHost::start_with_seat(
        "approval-scope",
        TTL,
        SeatKind::Team {
            turn_timeout: CLI_TIMEOUT,
            delegation_timeout: CLI_TIMEOUT,
            uac_plugin: None,
        },
    )
    .await;
    let office = office(&host);
    grant_all(&host, &office);
    let approval = ask(&host, &office, "npm install left-pad");
    let decide = json!({ "approval_id": approval.id, "decision": "deny" });

    // A worker never decides, whatever its level.
    let (status, body) = call(&host, "tok-specialist", "approval.decide", decide.clone()).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["detail"]["code"], "approval_scope_denied");
    assert_eq!(body["detail"]["field"], "approval_id");

    // A lead of another project does not decide this project's workers.
    let (status, body) = call(&host, "tok-other-lead", "approval.decide", decide).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["detail"]["code"], "approval_scope_denied");

    // Both refusals left the approval waiting, untouched.
    assert_eq!(
        host.chat.approval(&approval.id).expect("approval").status,
        ApprovalStatus::Pending
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_listing_is_scoped_to_the_callers_level() {
    let host = TestHost::start_with_seat(
        "approval-pending",
        TTL,
        SeatKind::Team {
            turn_timeout: CLI_TIMEOUT,
            delegation_timeout: CLI_TIMEOUT,
            uac_plugin: None,
        },
    )
    .await;
    let office = office(&host);
    grant_all(&host, &office);
    let approval = ask(&host, &office, "npm install left-pad");

    // The lead sees its project's request; the other project sees none.
    let (status, body) = call(&host, "tok-lead", "approval.pending", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(listed_ids(&body), vec![approval.id.clone()]);

    let (_, body) = call(&host, "tok-other-lead", "approval.pending", json!({})).await;
    assert!(listed_ids(&body).is_empty(), "{body}");

    // Alfred lists the company's requests at its level: none waits there.
    let (_, body) = call(&host, "tok-alfred", "approval.pending", json!({})).await;
    assert!(listed_ids(&body).is_empty(), "{body}");

    // A worker cannot list the decisions it has no part in.
    let (status, body) = call(&host, "tok-specialist", "approval.pending", json!({})).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["detail"]["code"], "approval_scope_denied");

    // A filter in the body is refused, not applied: the scope is the token's.
    let (status, body) = call(
        &host,
        "tok-lead",
        "approval.pending",
        json!({ "project_id": "prj_otro" }),
    )
    .await;
    assert_eq!(status, 422, "{body}");

    // Once decided, it no longer waits.
    call(
        &host,
        "tok-lead",
        "approval.decide",
        json!({ "approval_id": approval.id, "decision": "approve" }),
    )
    .await;
    let (_, body) = call(&host, "tok-lead", "approval.pending", json!({})).await;
    assert!(listed_ids(&body).is_empty(), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_host_stops_the_agents_it_started() {
    let host = TestHost::start_with_seat("drop-guard", TTL, SeatKind::Fake).await;
    let pane = host.spawn_fake(&[]).await;
    let pid = host.host.pid_of(&pane).expect("the agent has a process");
    assert!(process_exists(pid), "the agent runs while the host lives");

    drop(host);
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_exists(pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!process_exists(pid), "the agent outlived its host");
}
