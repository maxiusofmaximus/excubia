//! The UAC canary: the local provider that makes the preflight free, and the
//! evidence run against the real OpenCode binary.
//!
//! The evidence test is `#[ignore]`d: it needs the OpenCode V1 build the `e2e/`
//! harness of the plugin installs under `target/opencode-v1`, and it is run by
//! hand (`cargo test -p hn-core-seat --test uac_canary -- --ignored`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use hn_core_seat::http::{self, LoopbackUrl};
use hn_core_seat::{
    canary_plan, run_canary, shipped, CanaryProvider, CliId, CliProfile, UacState, CANARY_COMMAND,
    CANARY_MARKER,
};
use hn_core_test_dir::TestDir;

/// The real OpenCode of the harness, or whatever `HN_OPENCODE_BIN` names.
fn opencode() -> CliProfile {
    let mut profile = shipped()
        .into_iter()
        .find(|profile| profile.id == CliId::OpenCode)
        .expect("the shipped OpenCode profile");
    if let Ok(binary) = std::env::var("HN_OPENCODE_BIN") {
        if !binary.trim().is_empty() {
            profile.command = binary;
        }
    }
    profile
}

fn harness_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("opencode-v1")
        .join("node_modules")
        .join("opencode-windows-x64")
        .join("bin")
        .join("opencode.exe")
}

fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tooling")
        .join("opencode-uac-plugin")
}

/// A plugin that loads but has no hook: the one the `e2e/` harness uses for
/// the "it loaded and protected nothing" case.
fn plugin_without_a_hook() -> PathBuf {
    plugin_dir()
        .join("e2e")
        .join("fixtures")
        .join("plugin-sin-server")
}

async fn post(url: &LoopbackUrl, streaming: bool) -> String {
    let payload = serde_json::json!({
        "model": "mock/mock",
        "stream": streaming,
        "messages": [{ "role": "user", "content": "hola" }],
        "tools": [{ "type": "function", "function": { "name": "bash" } }],
    })
    .to_string();
    let response = http::request(
        url,
        "POST",
        &[("Content-Type", "application/json")],
        payload.as_bytes(),
        Duration::from_secs(5),
    )
    .await
    .expect("the provider answers");
    assert_eq!(response.status, 200);
    String::from_utf8_lossy(&response.body).to_string()
}

#[tokio::test]
async fn the_provider_asks_once_for_the_command_and_then_closes() {
    let provider = CanaryProvider::start(CANARY_COMMAND)
        .await
        .expect("provider");
    let url =
        LoopbackUrl::parse(&format!("{}/chat/completions", provider.base_url())).expect("url");

    let first = post(&url, false).await;
    let body: serde_json::Value = serde_json::from_str(&first).expect("json");
    assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");
    let arguments = body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .expect("arguments");
    let arguments: serde_json::Value = serde_json::from_str(arguments).expect("arguments json");
    assert_eq!(arguments["command"], CANARY_COMMAND);

    // The second turn ends the run: a loop of tool calls would hang the canary.
    let second = post(&url, false).await;
    let body: serde_json::Value = serde_json::from_str(&second).expect("json");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");

    // The streamed shape the SDKs use.
    let streamed = post(&url, true).await;
    assert!(streamed.contains("chat.completion.chunk"), "{streamed}");
    assert!(streamed.contains("[DONE]"), "{streamed}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real OpenCode binary against the local provider"]
async fn the_real_opencode_canary_is_safe_without_quota() {
    let mut profile = opencode();
    let binary = PathBuf::from(&profile.command);
    if !binary.is_file() {
        profile.command = harness_binary().to_string_lossy().to_string();
    }
    let binary = PathBuf::from(&profile.command);
    assert!(
        binary.is_file(),
        "no OpenCode binary: set HN_OPENCODE_BIN or install the e2e harness at {}",
        harness_binary().display()
    );
    let work = TestDir::new("canary-evidence");

    let plan = canary_plan(&profile, &plugin_dir(), &work, &work)
        .await
        .expect("the OpenCode canary plan");
    eprintln!("canary provider: {:?}", plan.provider_url());
    let outcome = run_canary(&plan, Duration::from_secs(120)).await;
    eprintln!("canary verdict: {:?} — {}", outcome.state, outcome.reason);
    assert_eq!(outcome.state, UacState::Safe, "{}", outcome.reason);
    assert!(
        !work.join(CANARY_MARKER).exists(),
        "the canary command ran: the plugin did not block it"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real OpenCode binary against a plugin without a hook"]
async fn the_real_opencode_canary_refuses_a_plugin_without_a_hook() {
    let mut profile = opencode();
    let binary = PathBuf::from(&profile.command);
    if !binary.is_file() {
        profile.command = harness_binary().to_string_lossy().to_string();
    }
    assert!(
        Path::new(&profile.command).is_file(),
        "no OpenCode binary: set HN_OPENCODE_BIN or install the e2e harness at {}",
        harness_binary().display()
    );
    let work = TestDir::new("canary-evidence-unprotected");

    let plan = canary_plan(&profile, &plugin_without_a_hook(), &work, &work)
        .await
        .expect("the OpenCode canary plan");
    let outcome = run_canary(&plan, Duration::from_secs(120)).await;
    eprintln!("canary verdict: {:?} — {}", outcome.state, outcome.reason);
    assert_eq!(outcome.state, UacState::Unsafe, "{}", outcome.reason);
    assert!(
        work.join(CANARY_MARKER).exists(),
        "the test command ran, which is why the canary is unsafe"
    );
}
