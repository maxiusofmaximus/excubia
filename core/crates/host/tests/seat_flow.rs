//! CORE6B acceptance: Alfred's real seat, the MCP shim and the simulated CLI.
//!
//! Every test uses `fake-cli` and `hn-mcp` as real child processes. No model
//! quota is spent; the simulated CLI speaks the MCP JSON-RPC protocol and the
//! host verifies the same restricted tool surface a real CLI would see.

mod harness;

use std::time::{Duration, Instant};

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_chat::RunState;
use hn_core_test_support::client::SignedClient;
use serde_json::Value;

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(20);

async fn paired_host(name: &str, seat: SeatKind) -> (TestHost, SignedClient) {
    let host = TestHost::start_with_seat(name, TTL, seat).await;
    let (client, paired) = host.pair("Desktop seat").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    (host, client)
}

async fn conversation(client: &SignedClient) -> String {
    client
        .chat_create("Seat", None)
        .await
        .expect("create conversation")["conversation"]["id"]
        .as_str()
        .expect("conversation id")
        .to_string()
}

async fn wait_for_entries(
    client: &SignedClient,
    conversation_id: &str,
    count: usize,
    timeout: Duration,
) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        let page = client
            .chat_timeline(conversation_id, &[("limit", "20")])
            .await
            .expect("timeline");
        let entries = page["entries"].as_array().cloned().unwrap_or_default();
        if entries.len() >= count {
            return page;
        }
        if Instant::now() >= deadline {
            panic!(
                "timeline stayed at {} entries (wanted {count}): {page}",
                entries.len()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_state(host: &TestHost, run_id: &str, wanted: RunState, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if host.chat.run_state(run_id).expect("run state") == Some(wanted) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "run {run_id} did not become {wanted:?}; last state {:?}",
                host.chat.run_state(run_id)
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn detail(body: &str) -> Value {
    serde_json::from_str::<Value>(body).expect("JSON error body")["detail"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_real_seat_answers_through_chat_reply_and_closes_the_run() {
    let (host, client) = paired_host(
        "seat-happy",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-happy", "hola Alfred")
        .await
        .expect("send");
    assert_eq!(accepted["turn"]["state"], "queued");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    let timeline = wait_for_entries(&client, &conversation_id, 2, CLI_TIMEOUT).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(entries[0]["author_kind"], "user");
    assert_eq!(entries[1]["author_kind"], "agent");
    assert_eq!(entries[1]["state"], "complete");
    assert!(
        entries[1]["preview"]
            .as_str()
            .expect("preview")
            .starts_with("Alfred: recibido"),
        "{timeline}"
    );
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Completed)
    );

    // The opening SSE frame carries the additive chat block, and the close
    // frame is an `updated` with `state: complete`, never a `created` only.
    let replay = client
        .events_with_last_id(1, Some(0))
        .await
        .expect("SSE replay");
    assert!(replay.contains("event: state"), "{replay}");
    assert!(replay.contains("\"chat\":{"), "{replay}");
    assert!(replay.contains("\"pending_approvals\":0"), "{replay}");
    assert!(replay.contains("\"change\":\"updated\""), "{replay}");
    assert!(replay.contains("\"state\":\"complete\""), "{replay}");

    let state = client.state().await.expect("state");
    assert!(
        state["chat"]["revision"].as_i64().unwrap_or_default() > 0,
        "{state}"
    );
    assert_eq!(state["chat"]["unread"], 1);
    assert_eq!(state["chat"]["pending_approvals"], 0);
    assert_eq!(state["chat"]["active_delegations"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_without_a_reply_fails_with_a_card_never_invented_text() {
    let (host, client) = paired_host(
        "seat-no-reply",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-silent", "[[fake:silent]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    let timeline = wait_for_entries(&client, &conversation_id, 2, CLI_TIMEOUT).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(entries[0]["author_kind"], "user");
    assert_eq!(entries[1]["author_kind"], "agent");
    assert_eq!(entries[1]["state"], "failed");
    assert_eq!(entries[1]["preview"], "");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Failed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mcp_422_path_is_a_failed_run_without_a_card() {
    let (host, client) = paired_host(
        "seat-invalid",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-invalid", "[[fake:invalid]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    let timeline = wait_for_entries(&client, &conversation_id, 2, CLI_TIMEOUT).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(entries[1]["author_kind"], "agent");
    assert_eq!(entries[1]["state"], "failed");
    assert_eq!(entries[1]["preview"], "");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Failed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_user_is_a_visible_message_and_the_turn_still_ends_with_reply() {
    let (host, client) = paired_host(
        "seat-ask",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-ask", "[[fake:ask]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    let timeline = wait_for_entries(&client, &conversation_id, 3, CLI_TIMEOUT).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(entries[0]["author_kind"], "user");
    assert_eq!(entries[1]["author_kind"], "agent");
    assert_eq!(entries[1]["preview"], "¿Qué proyecto?");
    assert_eq!(entries[2]["author_kind"], "agent");
    assert_eq!(entries[2]["preview"], "Alfred: pregunta enviada");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Completed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn injected_output_is_stored_as_data_and_unlocks_nothing() {
    let (host, client) = paired_host(
        "seat-inject",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-inject", "[[fake:inject]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    wait_for_entries(&client, &conversation_id, 2, CLI_TIMEOUT).await;
    let messages = client
        .chat_messages(&conversation_id, &[("limit", "10")])
        .await
        .expect("messages");
    let body = messages["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["author_kind"] == "agent")
        .and_then(|message| message["body_md"].as_str())
        .expect("agent body");
    assert!(body.contains("IGNORE ALL PREVIOUS INSTRUCTIONS"), "{body}");
    assert!(body.contains("<approval>"), "{body}");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Completed)
    );
    let state = client.state().await.expect("state");
    assert_eq!(state["chat"]["pending_approvals"], 0);
    assert_eq!(state["chat"]["active_delegations"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_queue_is_agent_busy_and_does_not_start_a_second_process() {
    let (host, client) = paired_host(
        "seat-busy",
        SeatKind::Cli {
            turn_timeout: Duration::from_secs(30),
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let first = client
        .chat_send(&conversation_id, "client-busy-1", "[[fake:hang]]")
        .await
        .expect("first send");
    let first_run = first["turn"]["id"].as_str().expect("run id").to_string();
    wait_for_state(&host, &first_run, RunState::Running, CLI_TIMEOUT).await;

    let second = client
        .chat_send(&conversation_id, "client-busy-2", "segundo")
        .await
        .expect("the queue has a free slot");
    assert_eq!(second["turn"]["state"], "queued");

    let third = client
        .chat_send(&conversation_id, "client-busy-3", "tercero")
        .await
        .expect_err("the queue is full");
    assert_eq!(third.status, 409);
    assert_eq!(detail(&third.body), "agent_busy");
    host.seat.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hung_cli_is_killed_by_the_turn_timeout_and_the_run_fails() {
    let (host, client) = paired_host(
        "seat-timeout",
        SeatKind::Cli {
            turn_timeout: Duration::from_millis(300),
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-timeout", "[[fake:hang]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();

    let timeline = wait_for_entries(&client, &conversation_id, 2, CLI_TIMEOUT).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(entries[1]["author_kind"], "agent");
    assert_eq!(entries[1]["state"], "failed");
    assert_eq!(entries[1]["preview"], "");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Failed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_interrupts_without_duplicating_the_turn() {
    let (host, client) = paired_host(
        "seat-restart",
        SeatKind::Cli {
            turn_timeout: Duration::from_secs(30),
        },
    )
    .await;
    let conversation_id = conversation(&client).await;
    let accepted = client
        .chat_send(&conversation_id, "client-restart", "[[fake:hang]]")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();
    wait_for_state(&host, &run_id, RunState::Running, CLI_TIMEOUT).await;

    let identity = client.identity().clone();
    let host = host
        .restart_with(
            TTL,
            SeatKind::Cli {
                turn_timeout: Duration::from_secs(20),
            },
        )
        .await;
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Interrupted)
    );
    let client = SignedClient::new(&host.base_url(), identity);
    let timeline = client
        .chat_timeline(&conversation_id, &[("limit", "10")])
        .await
        .expect("timeline");
    assert_eq!(timeline["entries"].as_array().expect("entries").len(), 1);

    // The original client_id is idempotent: it returns the old interrupted
    // turn instead of enqueueing the message again.
    let duplicate = client
        .chat_send(&conversation_id, "client-restart", "[[fake:hang]]")
        .await
        .expect("duplicate send");
    assert_eq!(duplicate["turn"]["id"], run_id.as_str());
    assert_eq!(duplicate["turn"]["state"], "interrupted");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let timeline = client
        .chat_timeline(&conversation_id, &[("limit", "10")])
        .await
        .expect("timeline after duplicate");
    assert_eq!(timeline["entries"].as_array().expect("entries").len(), 1);
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Interrupted)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_cli_is_seat_unavailable_and_leaves_no_card() {
    let (_host, client) = paired_host("seat-missing", SeatKind::CliMissing).await;
    let conversation_id = conversation(&client).await;
    let error = client
        .chat_send(&conversation_id, "client-missing", "hola")
        .await
        .expect_err("no CLI is installed");
    assert_eq!(error.status, 409);
    assert_eq!(detail(&error.body), "seat_unavailable");
    // A refused turn is withdrawn: the message does not stay in the timeline.
    let timeline = client
        .chat_timeline(&conversation_id, &[("limit", "10")])
        .await
        .expect("timeline");
    assert_eq!(timeline["entries"].as_array().expect("entries").len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_internal_tool_endpoint_rejects_missing_and_wrong_tokens() {
    let (host, _client) = paired_host(
        "seat-token",
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    let http = reqwest::Client::new();
    let body = serde_json::json!({
        "tool": "chat.reply",
        "arguments": { "text": "no debería entrar" }
    });

    let missing = http
        .post(&host.tool_url)
        .json(&body)
        .send()
        .await
        .expect("missing token request");
    assert_eq!(missing.status(), 401);
    let missing_body: Value = missing.json().await.expect("json");
    assert_eq!(missing_body["detail"]["code"], "invalid_tool_token");

    let wrong = http
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", "not-a-real-token")
        .json(&body)
        .send()
        .await
        .expect("wrong token request");
    assert_eq!(wrong.status(), 401);
    let wrong_body: Value = wrong.json().await.expect("json");
    assert_eq!(wrong_body["detail"]["code"], "invalid_tool_token");

    // The public listener never mounts the internal path.
    let public = http
        .post(format!("{}/internal/chat/tools", host.base_url()))
        .json(&body)
        .send()
        .await
        .expect("public request");
    assert_ne!(public.status(), 200);
}
