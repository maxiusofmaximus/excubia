//! The UAC plugin against the real OpenCode CLI, at host level (RFC
//! alfred-cli-engine (documento ausente), F-D). The seat runs OpenCode with the plugin, the local
//! provider asks it for one shell command, and the operator's answer decides
//! whether that command runs.
//!
//! The tests are `#[ignore]`d: they need the OpenCode binary (`HN_OPENCODE_BIN`,
//! or the shipped command on the PATH) and the plugin under `tooling/`. Run with
//! `cargo test -p hn-core-host --test opencode_uac -- --ignored`.

mod harness;

use std::path::{absolute, Path, PathBuf};
use std::time::Duration;

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_chat::RunState;
use hn_core_seat::{canary_command, CanaryProvider};
use hn_core_test_dir::TestDir;
use hn_core_test_support::SignedClient;
use serde_json::Value;
use tokio::time::{sleep, Instant};

/// The approval TTL the operator is given in the approve and deny cases.
const TTL: Duration = Duration::from_secs(45);
/// OpenCode starts slowly on a cold cache: the turn gets more than the unit tests.
const CLI_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tooling")
        .join("opencode-uac-plugin")
}

/// A host whose Alfred is the real OpenCode with the plugin, answered by a local
/// provider that asks once for `command` (the provider stays alive with the test).
async fn opencode_host(
    name: &str,
    ttl: Duration,
    command: &str,
    approval_ttl_seconds: u64,
) -> (TestHost, SignedClient, String, CanaryProvider) {
    let provider = CanaryProvider::start(command).await.expect("provider");
    let seat = SeatKind::OpenCode {
        turn_timeout: CLI_TIMEOUT,
        plugin_dir: plugin_dir(),
        provider_url: provider.base_url().to_string(),
        approval_ttl_seconds,
    };
    let host = TestHost::start_with_seat(name, ttl, seat).await;
    let (client, paired) = host.pair("Desktop opencode").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    (host, client, paired.device_id, provider)
}

/// Sends one message and returns the run's id.
async fn send(client: &SignedClient, name: &str) -> String {
    let conversation = client.chat_create(name, None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let accepted = client
        .chat_send(&conversation_id, "client-opencode", "haz la tarea")
        .await
        .expect("send");
    accepted["turn"]["id"].as_str().expect("run id").to_string()
}

/// The first approval of the inbox with `status`, once it appears.
async fn wait_for_approval(client: &SignedClient, status: &str, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        let inbox = client
            .chat_approvals(&[("status", status)])
            .await
            .expect("inbox");
        if let Some(first) = inbox["approvals"].as_array().and_then(|rows| rows.first()) {
            return first.clone();
        }
        assert!(
            Instant::now() < deadline,
            "no approval with status {status}"
        );
        sleep(POLL_INTERVAL).await;
    }
}

/// Waits until the run leaves its active states and returns the final one.
async fn wait_for_run_end(host: &TestHost, run_id: &str, timeout: Duration) -> RunState {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state @ (RunState::Completed | RunState::Failed | RunState::Interrupted)) =
            host.chat.run_state(run_id).expect("run state")
        {
            return state;
        }
        assert!(Instant::now() < deadline, "the run did not end");
        sleep(POLL_INTERVAL).await;
    }
}

async fn consumed_count(client: &SignedClient) -> usize {
    client
        .chat_approvals(&[("status", "consumed")])
        .await
        .expect("inbox")["approvals"]
        .as_array()
        .map_or(0, Vec::len)
}

/// The absolute path of a marker in `work`: the CLI runs shell commands in its own
/// project directory, so a relative marker would not prove anything.
fn marker_in(work: &Path) -> PathBuf {
    absolute(work.join("opencode-marker.txt")).expect("absolute marker")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real OpenCode binary with the UAC plugin"]
async fn an_approved_command_runs_once_through_the_real_opencode() {
    let work = TestDir::new("opencode-uac-approve");
    let marker = marker_in(&work);
    let command = canary_command(&marker);
    let (host, client, device, _provider) =
        opencode_host("opencode-approve", TTL, &command, 45).await;

    let run_id = send(&client, "Permiso opencode").await;
    let approval = wait_for_approval(&client, "pending", CLI_TIMEOUT).await;
    let approval_id = approval["id"].as_str().expect("id").to_string();
    assert_eq!(approval["kind"], "command");

    let decided = client
        .chat_decide_approval(&approval_id, "approve", &device)
        .await
        .expect("decision");
    assert_eq!(decided["approval"]["status"], "approved");

    // The canary provider never calls `chat.reply`, so the seat ends the run as
    // failed even when the gate works: the run state is not the proof here.
    wait_for_run_end(&host, &run_id, CLI_TIMEOUT).await;
    // The positive control: the command ran, so the denial and TTL cases below
    // prove the gate and not a provider that never asked for it.
    assert!(marker.exists(), "the approved command did not run");
    assert_eq!(consumed_count(&client).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real OpenCode binary with the UAC plugin"]
async fn a_denied_command_does_not_run_through_the_real_opencode() {
    let work = TestDir::new("opencode-uac-deny");
    let marker = marker_in(&work);
    let command = canary_command(&marker);
    let (host, client, device, _provider) =
        opencode_host("opencode-deny", TTL, &command, 45).await;

    let run_id = send(&client, "Permiso opencode").await;
    let approval = wait_for_approval(&client, "pending", CLI_TIMEOUT).await;
    let approval_id = approval["id"].as_str().expect("id").to_string();

    let decided = client
        .chat_decide_approval(&approval_id, "deny", &device)
        .await
        .expect("decision");
    assert_eq!(decided["approval"]["status"], "denied");

    wait_for_run_end(&host, &run_id, CLI_TIMEOUT).await;
    assert!(!marker.exists(), "a denied command ran");
    assert_eq!(consumed_count(&client).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real OpenCode binary with the UAC plugin"]
async fn a_command_without_an_answer_expires_and_does_not_run() {
    let work = TestDir::new("opencode-uac-ttl");
    let marker = marker_in(&work);
    let command = canary_command(&marker);
    // The host and the plugin both wait 3 s: nobody answers.
    let ttl = Duration::from_secs(3);
    let (host, client, _device, _provider) =
        opencode_host("opencode-ttl", ttl, &command, 3).await;

    let run_id = send(&client, "Permiso opencode").await;
    wait_for_approval(&client, "pending", CLI_TIMEOUT).await;
    wait_for_approval(&client, "expired", CLI_TIMEOUT).await;

    wait_for_run_end(&host, &run_id, CLI_TIMEOUT).await;
    assert!(!marker.exists(), "an expired command ran");
    assert_eq!(consumed_count(&client).await, 0);
}
