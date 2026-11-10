//! Alfred's model switch (`alfred-local`) on the real seat. A turn's CLI sees the local
//! model's environment only while the marker is there, and never the API key. The CLI is
//! `fake-cli`: it writes the names of the variables it sees to `FAKE_CLI_ENV_OUT`.

mod harness;

use std::time::{Duration, Instant};

use std::sync::atomic::Ordering;

use harness::{SeatKind, TestHost, QUOTA_CALLS};
use excubia_core_auth::Permission;
use excubia_core_chat::RunState;
use excubia_core_host::alfred_backend;
use excubia_core_test_support::client::SignedClient;

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(20);
const LOCAL_NAMES: [&str; 5] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
];

/// Sends one message, waits for its run to close, and returns the variable names its CLI saw.
async fn turn(
    host: &TestHost,
    client: &SignedClient,
    conversation_id: &str,
    client_id: &str,
) -> Vec<String> {
    let report = host.data_dir.join("fake-env.txt");
    let _ = std::fs::remove_file(&report);
    let accepted = client
        .chat_send(conversation_id, client_id, "[[fake:env]] ¿Qué ves?")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();
    let deadline = Instant::now() + CLI_TIMEOUT;
    while host.chat.run_state(&run_id).expect("run state") != Some(RunState::Completed) {
        assert!(Instant::now() < deadline, "the turn did not complete");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    std::fs::read_to_string(&report)
        .expect("the CLI wrote its report")
        .split(',')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

async fn new_conversation(client: &SignedClient, title: &str) -> String {
    client
        .chat_create(title, None)
        .await
        .expect("create conversation")["conversation"]["id"]
        .as_str()
        .expect("conversation id")
        .to_string()
}

/// Sends one message and waits until its run is over; returns the state it ended in.
async fn finished_run(
    host: &TestHost,
    client: &SignedClient,
    conversation_id: &str,
    client_id: &str,
    text: &str,
) -> Option<RunState> {
    let accepted = client
        .chat_send(conversation_id, client_id, text)
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let state = host.chat.run_state(&run_id).expect("run state");
        if matches!(
            state,
            Some(RunState::Completed | RunState::Failed | RunState::Interrupted)
        ) {
            return state;
        }
        assert!(Instant::now() < deadline, "the turn did not end");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_marker_decides_each_turns_model_and_the_api_key_leaves_with_it() {
    // The seat's CLI inherits this process's environment. This binary has no other test.
    std::env::set_var("ANTHROPIC_API_KEY", "sk-test-not-a-real-key");
    let host = TestHost::start_with_seat(
        "alfred-local",
        TTL,
        SeatKind::Cli {
            turn_timeout: CLI_TIMEOUT,
        },
    )
    .await;
    std::env::set_var("FAKE_CLI_ENV_OUT", host.data_dir.join("fake-env.txt"));
    let (client, paired) = host.pair("Desktop alfred").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let conversation_id = client
        .chat_create("Alfred local", None)
        .await
        .expect("create conversation")["conversation"]["id"]
        .as_str()
        .expect("conversation id")
        .to_string();

    // No marker: Claude's own environment, API key included.
    let claude = turn(&host, &client, &conversation_id, "client-claude").await;
    assert!(
        claude.iter().any(|name| name == "ANTHROPIC_API_KEY"),
        "{claude:?}"
    );
    assert!(
        !claude.iter().any(|name| name == "ANTHROPIC_BASE_URL"),
        "{claude:?}"
    );

    // Marker: the local model's variables, and no API key although the host has one.
    alfred_backend::set(&host.data_dir).expect("set the marker");
    let local = turn(&host, &client, &conversation_id, "client-local").await;
    for name in LOCAL_NAMES {
        assert!(
            local.iter().any(|seen| seen == name),
            "{name} missing: {local:?}"
        );
    }
    assert!(
        !local.iter().any(|name| name == "ANTHROPIC_API_KEY"),
        "{local:?}"
    );

    // Marker gone: the next turn is Claude's again.
    alfred_backend::clear(&host.data_dir).expect("clear the marker");
    let back = turn(&host, &client, &conversation_id, "client-back").await;
    assert!(
        back.iter().any(|name| name == "ANTHROPIC_API_KEY"),
        "{back:?}"
    );
    assert!(
        !back.iter().any(|name| name == "ANTHROPIC_BASE_URL"),
        "{back:?}"
    );

    // Claude's quota message reaches the seat hook once. A plain failure does not. Each case
    // has its own conversation: a turn's CLI reads the whole context, earlier markers included.
    let calls = QUOTA_CALLS.load(Ordering::SeqCst);
    let plain_conversation = new_conversation(&client, "Alfred fallo").await;
    let plain = finished_run(
        &host,
        &client,
        &plain_conversation,
        "client-plain",
        "[[fake:silent]] Hola",
    )
    .await;
    assert_eq!(plain, Some(RunState::Failed));
    assert_eq!(
        QUOTA_CALLS.load(Ordering::SeqCst),
        calls,
        "a plain failure called the hook"
    );
    let quota_conversation = new_conversation(&client, "Alfred cuota").await;
    let quota = finished_run(
        &host,
        &client,
        &quota_conversation,
        "client-quota",
        "[[fake:quota]] Hola",
    )
    .await;
    assert_eq!(quota, Some(RunState::Failed));
    assert_eq!(
        QUOTA_CALLS.load(Ordering::SeqCst),
        calls + 1,
        "the quota failure did not call the hook"
    );

    // On the local model the same message is not Claude's quota: the hook stays silent.
    alfred_backend::set(&host.data_dir).expect("set the marker");
    let local_conversation = new_conversation(&client, "Alfred cuota local").await;
    let local = finished_run(
        &host,
        &client,
        &local_conversation,
        "client-quota-local",
        "[[fake:quota]] Hola",
    )
    .await;
    assert_eq!(local, Some(RunState::Failed));
    assert_eq!(
        QUOTA_CALLS.load(Ordering::SeqCst),
        calls + 1,
        "a local failure called the hook"
    );
    alfred_backend::clear(&host.data_dir).expect("clear the marker");
}
