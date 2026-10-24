//! CORE7 acceptance at HTTP level: the team flow (Alfred -> lead ->
//! specialist), the cards, the timeout, the cancellation and the typed
//! refusals of the delegation surface.
//!
//! Every test drives real `fake-cli` and `hn-mcp` processes. No model quota
//! is spent: the simulated CLI speaks the MCP protocol and the host enforces
//! the same rules a real CLI would meet.

mod harness;

use std::time::{Duration, Instant};

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_chat::DelegationState;
use hn_core_test_support::client::SignedClient;
use serde_json::Value;

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

fn team_seat(delegation_timeout: Duration) -> SeatKind {
    SeatKind::Team {
        turn_timeout: CLI_TIMEOUT,
        delegation_timeout,
        uac_plugin: None,
    }
}

async fn paired_host(name: &str, seat: SeatKind) -> (TestHost, SignedClient) {
    let host = TestHost::start_with_seat(name, TTL, seat).await;
    let (client, paired) = host.pair("Desktop team").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    (host, client)
}

async fn conversation(client: &SignedClient) -> String {
    client
        .chat_create("Equipo", None)
        .await
        .expect("create conversation")["conversation"]["id"]
        .as_str()
        .expect("conversation id")
        .to_string()
}

fn detail(body: &str) -> Value {
    serde_json::from_str::<Value>(body).expect("JSON body")["detail"].clone()
}

/// The delegation card of a conversation, once it exists.
async fn wait_for_card(client: &SignedClient, conversation_id: &str, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        let page = client
            .chat_timeline(conversation_id, &[("limit", "50")])
            .await
            .expect("timeline");
        let card = page["entries"]
            .as_array()
            .and_then(|entries| entries.iter().find(|entry| entry["kind"] == "card"))
            .cloned();
        if let Some(card) = card {
            return card;
        }
        assert!(Instant::now() < deadline, "no card appeared: {page}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until the delegation reaches one of the wanted states.
async fn wait_for_state(
    host: &TestHost,
    delegation_id: &str,
    wanted: &[DelegationState],
    timeout: Duration,
) -> DelegationState {
    let deadline = Instant::now() + timeout;
    loop {
        let delegation = host.chat.delegation(delegation_id).expect("delegation");
        if wanted.contains(&delegation.state) {
            return delegation.state;
        }
        assert!(
            Instant::now() < deadline,
            "delegation {delegation_id} stayed {:?}",
            delegation.state
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn alfred_creates_the_project_and_the_lead_delegates_to_a_specialist() {
    let (host, client) = paired_host("team-flow", team_seat(CLI_TIMEOUT)).await;
    let conversation_id = conversation(&client).await;
    client
        .chat_send(&conversation_id, "client-team", "[[fake:team]] empieza")
        .await
        .expect("send");

    let card = wait_for_card(&client, &conversation_id, CLI_TIMEOUT).await;
    let delegation_id = card["delegation_id"].as_str().expect("id").to_string();
    assert_eq!(card["kind"], "card");
    assert_eq!(card["ref_id"], delegation_id.as_str());
    let links = &card["links"];
    assert_eq!(
        links["delegation"],
        format!("/v1/chat/delegations/{delegation_id}").as_str()
    );
    let agent_link = links["agent"].as_str().expect("agent link");
    assert!(agent_link.starts_with("/v1/agents/"), "{agent_link}");

    // The delegation detail is a first-class resource.
    let (status, body) = client
        .request(
            "GET",
            &format!("/v1/chat/delegations/{delegation_id}"),
            None,
        )
        .await
        .expect("detail");
    assert_eq!(status, 200, "{body}");
    let detail = serde_json::from_str::<Value>(&body).expect("json");
    assert_eq!(detail["delegation"]["id"], delegation_id.as_str());
    assert_eq!(detail["delegation"]["depth"], 1);
    assert_eq!(detail["delegation"]["from_name"], "Alfred");
    assert_eq!(detail["delegation"]["to_name"], "Ernesto");

    // The lead spawns a specialist and delegates to it: one child, depth two.
    let child_id = wait_for_child(&host, &delegation_id, CLI_TIMEOUT).await;
    let child = host.chat.delegation(&child_id).expect("child");
    assert_eq!(child.depth, 2);
    assert_eq!(child.from_name, "Ernesto");
    assert_eq!(child.to_name, "Ana");
    assert_eq!(child.parent_id.as_deref(), Some(delegation_id.as_str()));

    // Both delegations end done and the result is a message of the seat.
    wait_for_state(&host, &child_id, &[DelegationState::Done], CLI_TIMEOUT).await;
    wait_for_state(&host, &delegation_id, &[DelegationState::Done], CLI_TIMEOUT).await;
    let detail = host
        .chat
        .delegation_detail(&child_id)
        .expect("child detail");
    let message = detail.result_message.expect("result message");
    assert_eq!(
        message.author_agent_id.as_deref(),
        Some(child.to_agent_id.as_str())
    );
    assert!(message.body_md.contains("Implementa"), "{message:?}");

    // The project and its lead are visible in the bootstrap.
    let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
    let projects = bootstrap["projects"].as_array().expect("projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "Demo");
    assert_eq!(projects[0]["lead"]["name"], "Ernesto");
    assert_eq!(projects[0]["lead"]["status"], "idle");
    assert!(
        projects[0]["root_label"]
            .as_str()
            .is_some_and(|label| label.ends_with("demo")),
        "{projects:?}"
    );
}

/// Wait until the lead has delegated to a specialist.
async fn wait_for_child(host: &TestHost, delegation_id: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let detail = host.chat.delegation_detail(delegation_id).expect("detail");
        if let Some(child) = detail.children.first() {
            return child.id.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the lead never delegated: {detail:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delegation_that_never_ends_fails_with_its_timeout() {
    let (host, client) = paired_host("team-timeout", team_seat(Duration::from_secs(2))).await;
    let conversation_id = conversation(&client).await;
    client
        .chat_send(
            &conversation_id,
            "client-hang",
            "[[fake:team]] [[fake:hang]]",
        )
        .await
        .expect("send");
    let card = wait_for_card(&client, &conversation_id, CLI_TIMEOUT).await;
    let delegation_id = card["delegation_id"].as_str().expect("id").to_string();

    let state = wait_for_state(
        &host,
        &delegation_id,
        &[DelegationState::Failed],
        CLI_TIMEOUT,
    )
    .await;
    assert_eq!(state, DelegationState::Failed);
    let delegation = host.chat.delegation(&delegation_id).expect("delegation");
    assert_eq!(delegation.reason.as_deref(), Some("delegation_timeout"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_delegation_keeps_its_history() {
    let (host, client) = paired_host("team-cancel", team_seat(CLI_TIMEOUT)).await;
    let conversation_id = conversation(&client).await;
    client
        .chat_send(
            &conversation_id,
            "client-silent",
            "[[fake:team]] [[fake:silent]]",
        )
        .await
        .expect("send");
    let card = wait_for_card(&client, &conversation_id, CLI_TIMEOUT).await;
    let delegation_id = card["delegation_id"].as_str().expect("id").to_string();
    wait_for_state(
        &host,
        &delegation_id,
        &[DelegationState::Running],
        CLI_TIMEOUT,
    )
    .await;

    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/chat/delegations/{delegation_id}/cancel"),
            None,
        )
        .await
        .expect("cancel");
    assert_eq!(status, 200, "{body}");
    let cancelled = serde_json::from_str::<Value>(&body).expect("json");
    assert_eq!(cancelled["delegation"]["state"], "cancelled");
    assert_eq!(cancelled["card"]["status"], "cancelled");

    // A second cancellation is a typed 409, not a silent success.
    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/chat/delegations/{delegation_id}/cancel"),
            None,
        )
        .await
        .expect("cancel again");
    assert_eq!(status, 409, "{body}");
    assert_eq!(detail(&body), "delegation_cancelled");

    // The card and the history are still there.
    let page = client
        .chat_timeline(&conversation_id, &[("limit", "50")])
        .await
        .expect("timeline");
    assert!(page["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .any(|entry| entry["delegation_id"] == delegation_id.as_str()));

    // The kill switch of the conversation answers with the cancelled ids.
    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/cancel"),
            None,
        )
        .await
        .expect("kill switch");
    assert_eq!(status, 200, "{body}");
    let answer = serde_json::from_str::<Value>(&body).expect("json");
    assert_eq!(answer["conversation_id"], conversation_id.as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_delegation_is_a_404_and_a_missing_one_is_not_a_panic() {
    let (_host, client) = paired_host("team-404", team_seat(CLI_TIMEOUT)).await;
    let (status, body) = client
        .request("GET", "/v1/chat/delegations/dlg_nope", None)
        .await
        .expect("detail");
    assert_eq!(status, 404, "{body}");
    assert_eq!(detail(&body), "no_such_delegation");
}

#[tokio::test(flavor = "multi_thread")]
async fn six_live_panels_stop_the_seventh() {
    let (host, client) = paired_host("team-panes", team_seat(CLI_TIMEOUT)).await;
    // Six panels are already alive: the lead of a new project cannot start.
    for _ in 0..6 {
        host.spawn_fake(&[]).await;
    }
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-panes", "[[fake:team]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    // The turn fails: the lead panel was refused.
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let state = host.chat.run_state(&run_id).expect("run state");
        if state.is_some_and(|state| state == hn_core_chat::RunState::Failed) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the turn never failed: {state:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
    let projects = bootstrap["projects"].as_array().expect("projects");
    assert_eq!(projects.len(), 1, "the project exists");
    assert!(
        projects[0]["lead"]["pane_id"].is_null(),
        "the lead has no panel: {projects:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_interrupts_the_running_delegations_without_relaunching_them() {
    let (host, client) = paired_host("team-restart", team_seat(CLI_TIMEOUT)).await;
    let conversation_id = conversation(&client).await;
    client
        .chat_send(
            &conversation_id,
            "client-restart",
            "[[fake:team]] [[fake:silent]]",
        )
        .await
        .expect("send");
    let card = wait_for_card(&client, &conversation_id, CLI_TIMEOUT).await;
    let delegation_id = card["delegation_id"].as_str().expect("id").to_string();
    wait_for_state(
        &host,
        &delegation_id,
        &[DelegationState::Running],
        CLI_TIMEOUT,
    )
    .await;

    let identity = client.identity().clone();
    let host = host.restart_with(TTL, team_seat(CLI_TIMEOUT)).await;
    let delegation = host.chat.delegation(&delegation_id).expect("delegation");
    assert_eq!(delegation.state, DelegationState::Interrupted);
    assert_eq!(delegation.reason.as_deref(), Some("host_restart"));

    // No panel is relaunched by itself.
    let client = SignedClient::new(&host.base_url(), identity);
    let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
    let projects = bootstrap["projects"].as_array().expect("projects");
    assert!(projects[0]["lead"]["pane_id"].is_null(), "{projects:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_mcp_call_is_a_422_with_its_code_and_field() {
    let (host, _client) = paired_host("team-422", SeatKind::Tools).await;
    let http = reqwest::Client::new();
    let body = serde_json::json!({
        "tool": "delegation.assign",
        "arguments": { "task": "sin destino" }
    });
    let response = http
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", harness::TOOL_TOKEN)
        .json(&body)
        .send()
        .await
        .expect("tool call");
    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["detail"]["code"], "invalid_arguments");
    assert_eq!(body["detail"]["field"], "to_agent_id");

    // An unknown tool is refused with its own code.
    let body = serde_json::json!({
        "tool": "shell.exec",
        "arguments": { "command": "rm -rf /" }
    });
    let response = http
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", harness::TOOL_TOKEN)
        .json(&body)
        .send()
        .await
        .expect("tool call");
    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["detail"]["code"], "unknown_tool");

    // A wrong token is not a 422.
    let body = serde_json::json!({ "tool": "chat.reply", "arguments": { "text": "hola" } });
    let response = http
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", "not-a-real-token")
        .json(&body)
        .send()
        .await
        .expect("tool call");
    assert_eq!(response.status(), 401);
}
