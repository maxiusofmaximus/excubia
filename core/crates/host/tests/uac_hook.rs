//! The blocking hook and the preflight of the plugin, at host level.
//!
//! The simulated CLI runs the same hook binary a real CLI would run before a
//! risky action, and the host refuses to launch a seat whose UAC adapter
//! could not be proven.

mod harness;

use std::path::PathBuf;
use std::time::Duration;

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_test_dir::TestDir;
use hn_core_test_support::SignedClient;
use serde_json::Value;
use tokio::time::{sleep, Instant};

const TTL: Duration = Duration::from_secs(45);
const CLI_TIMEOUT: Duration = Duration::from_secs(30);
/// Every poll of the inbox costs one of the 120 reads per minute of the
/// rate limiter: 500 ms keeps a 30 s wait far below that budget.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A plugin directory as OpenCode would load it. `loads` says whether the
/// module declares the hook the adapter needs.
fn plugin_dir(name: &str, loads: bool) -> TestDir {
    let dir = TestDir::new(&format!("uac-plugin-tests-{name}"));
    let index = if loads {
        r#"export const PLUGIN_ID = "herdr.uac";
export default { id: PLUGIN_ID, async setup(ctx) { await ctx.tool.hook("execute.before", () => {}); } };
"#
    } else {
        r#"export default { id: "broken" };
"#
    };
    std::fs::write(dir.join("index.js"), index).expect("index.js");
    dir
}

fn team_seat(uac_plugin: Option<PathBuf>) -> SeatKind {
    SeatKind::Team {
        turn_timeout: CLI_TIMEOUT,
        delegation_timeout: CLI_TIMEOUT,
        uac_plugin,
    }
}

async fn host_with_client(name: &str, seat: SeatKind) -> (TestHost, SignedClient, String) {
    let host = TestHost::start_with_seat(name, TTL, seat).await;
    let (client, paired) = host.pair("Desktop uac").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    (host, client, paired.device_id)
}

/// The pending approval of the inbox, once it appears.
async fn wait_for_pending(client: &SignedClient, timeout: Duration) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        let inbox = client
            .chat_approvals(&[("status", "pending")])
            .await
            .expect("inbox");
        if let Some(first) = inbox["approvals"].as_array().and_then(|rows| rows.first()) {
            return first.clone();
        }
        assert!(Instant::now() < deadline, "no approval appeared");
        sleep(POLL_INTERVAL).await;
    }
}

/// The plugin of the test: the good one loads and the bad one does not.
fn plugin_with_hook(name: &str) -> TestDir {
    plugin_dir(name, true)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hook_waits_for_the_operator_and_the_action_runs_only_once() {
    let (_host, client, device) = host_with_client("uac-hook", team_seat(None)).await;
    let conversation = client.chat_create("Permiso", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    client
        .chat_send(&conversation_id, "client-uac", "[[fake:team]] [[fake:uac]]")
        .await
        .expect("send");

    // The hook opened an approval and waits for the operator.
    let approval = wait_for_pending(&client, CLI_TIMEOUT).await;
    let approval_id = approval["id"].as_str().expect("id").to_string();
    assert_eq!(approval["kind"], "command");
    assert_eq!(approval["risk"], "high");

    // The operator allows it.
    let decided = client
        .chat_decide_approval(&approval_id, "approve", &device)
        .await
        .expect("decision");
    assert_eq!(decided["approval"]["status"], "approved");

    // The hook consumes it and the seat finishes the task.
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let inbox = client
            .chat_approvals(&[("status", "consumed")])
            .await
            .expect("inbox");
        if let Some(first) = inbox["approvals"].as_array().and_then(|rows| rows.first()) {
            assert_eq!(first["id"], approval_id.as_str());
            assert_eq!(first["audit"][0]["device_id"], device.as_str());
            assert_eq!(first["audit"][1]["decision"], "consumed");
            break;
        }
        assert!(Instant::now() < deadline, "the approval was not consumed");
        sleep(POLL_INTERVAL).await;
    }
    assert_eq!(
        client.state().await.expect("state")["chat"]["pending_approvals"],
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_action_does_not_run() {
    let (_host, client, device) = host_with_client("uac-denied", team_seat(None)).await;
    let conversation = client.chat_create("Permiso", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    client
        .chat_send(&conversation_id, "client-uac", "[[fake:team]] [[fake:uac]]")
        .await
        .expect("send");

    let approval = wait_for_pending(&client, CLI_TIMEOUT).await;
    let approval_id = approval["id"].as_str().expect("id").to_string();
    let denied = client
        .chat_decide_approval(&approval_id, "deny", &device)
        .await
        .expect("decision");
    assert_eq!(denied["approval"]["status"], "denied");

    // The approval is never consumed: the action did not run.
    sleep(Duration::from_millis(500)).await;
    let inbox = client
        .chat_approvals(&[("status", "consumed")])
        .await
        .expect("inbox");
    assert_eq!(
        inbox["approvals"].as_array().map(Vec::len),
        Some(0),
        "a denied approval is not consumed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_that_nobody_answers_denies_and_the_action_never_runs() {
    let (_host, client, _device) = host_with_client("uac-timeout", team_seat(None)).await;
    let conversation = client.chat_create("Permiso", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    // The simulated CLI asks for a one-second approval and nobody decides it.
    client
        .chat_send(
            &conversation_id,
            "client-uac-timeout",
            "[[fake:team]] [[fake:uac-ttl=1]]",
        )
        .await
        .expect("send");

    // The hook waits, gives up and answers 2: the approval expires and is
    // never consumed, which is what "the action did not run" means here.
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let expired = client
            .chat_approvals(&[("status", "expired")])
            .await
            .expect("inbox");
        if let Some(first) = expired["approvals"]
            .as_array()
            .and_then(|rows| rows.first())
        {
            assert_eq!(first["kind"], "command");
            assert_eq!(first["status"], "expired");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the approval never expired after the hook's timeout"
        );
        sleep(POLL_INTERVAL).await;
    }
    let consumed = client
        .chat_approvals(&[("status", "consumed")])
        .await
        .expect("inbox");
    assert_eq!(
        consumed["approvals"].as_array().map(Vec::len),
        Some(0),
        "a timed-out hook never lets the action run"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_that_expires_at_the_operator_exits_the_hook_with_two() {
    let (_host, client, _device) = host_with_client("uac-expiry", team_seat(None)).await;
    let conversation = client.chat_create("Permiso", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    // A one-second approval goes straight to the operator (CORE9-D): nobody
    // answers it, so it expires there and the hook must exit 2.
    client
        .chat_send(
            &conversation_id,
            "client-uac-expiry",
            "[[fake:team]] [[fake:uac-ttl=1]]",
        )
        .await
        .expect("send");

    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let expired = client
            .chat_approvals(&[("status", "expired")])
            .await
            .expect("inbox");
        if let Some(first) = expired["approvals"]
            .as_array()
            .and_then(|rows| rows.first())
        {
            assert_eq!(first["level"], "user", "a short TTL goes to the operator");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the approval never expired at the operator"
        );
        sleep(POLL_INTERVAL).await;
    }

    // The fake CLI reports the hook's exit: 2 is a denial, never "allowed".
    loop {
        let timeline = client
            .chat_timeline(&conversation_id, &[])
            .await
            .expect("timeline");
        if timeline.to_string().contains("uac denied") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the hook did not exit 2 on the expiry: {timeline}"
        );
        sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seat_whose_plugin_loaded_is_safe() {
    let plugin = plugin_with_hook("good");
    let (host, client, _device) =
        host_with_client("uac-preflight-ok", team_seat(Some(plugin.to_path_buf()))).await;
    let conversation = client.chat_create("Preflight", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    client
        .chat_send(&conversation_id, "client-preflight", "[[fake:team]]")
        .await
        .expect("send");

    // The lead of the project is verified and it has a panel.
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
        let projects = bootstrap["projects"].as_array().expect("projects");
        if let Some(lead) = projects
            .first()
            .and_then(|project| project.get("lead"))
            .filter(|lead| lead.is_object())
        {
            if lead["pane_id"].is_string() {
                assert_eq!(lead["uac_state"], "safe");
                let _ = host;
                break;
            }
        }
        assert!(Instant::now() < deadline, "the lead never got a panel");
        sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seat_whose_plugin_did_not_load_is_unsafe_and_is_not_launched() {
    let plugin = plugin_dir("bad", false);
    let (_host, client, _device) =
        host_with_client("uac-preflight-ko", team_seat(Some(plugin.to_path_buf()))).await;
    let conversation = client.chat_create("Preflight", None).await.expect("create");
    let conversation_id = conversation["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    client
        .chat_send(&conversation_id, "client-preflight", "[[fake:team]]")
        .await
        .expect("send");

    // The project exists but its lead is unsafe and has no panel.
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
        let projects = bootstrap["projects"].as_array().expect("projects");
        if let Some(lead) = projects
            .first()
            .and_then(|project| project.get("lead"))
            .filter(|lead| lead.is_object())
        {
            if lead["uac_state"] == "unsafe" {
                assert!(lead["pane_id"].is_null(), "an unsafe seat has no panel");
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the seat was never marked unsafe: {bootstrap}"
        );
        sleep(POLL_INTERVAL).await;
    }
}
