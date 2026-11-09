//! The engine switch over `/v1/alfred/backend` (ALFRED-BACKEND): the answers, the permissions, one
//! change at a time, a failed start that keeps Claude and retries, the voice given back on the way
//! to Claude, and a state read from the markers after a restart. The model is a double
//! (`FakeEngine` in the harness): nothing is launched and no Strata is touched.

mod harness;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use harness::{FakeEngine, TestHost};
use hn_core_api::{AlfredBackend, EngineListing};
use hn_core_auth::Permission;
use hn_core_chat::{ChatStore, ConversationFilter};
use hn_core_host::alfred_backend;
use hn_core_host::alfred_switch::{Engine, Switch};
use hn_core_host::local_mode;
use hn_core_host::pairing_notice::{PairingNotice, NOTICE_TITLE};
use hn_core_test_support::client::SignedClient;
use serde_json::Value;

const PATH: &str = "/v1/alfred/backend";
const WAIT: Duration = Duration::from_secs(10);

async fn paired(host: &TestHost, name: &str, grants: &[Permission]) -> SignedClient {
    let (client, paired) = host.pair(name).await;
    for permission in grants {
        host.grant(&paired.device_id, *permission);
    }
    client
}

async fn read(client: &SignedClient) -> (u16, Value) {
    let (status, body) = client.request("GET", PATH, None).await.expect("get");
    (status, serde_json::from_str(&body).expect("json"))
}

async fn ask(client: &SignedClient, backend: &str) -> (u16, Value) {
    let body = format!("{{\"backend\":\"{backend}\"}}").into_bytes();
    let (status, text) = client
        .request("POST", PATH, Some(body))
        .await
        .expect("post");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Reads the status until it shows `state`; fails after `WAIT`.
async fn wait_for(client: &SignedClient, state: &str) -> Value {
    let deadline = Instant::now() + WAIT;
    loop {
        let (_, body) = read(client).await;
        if body["state"] == state {
            return body;
        }
        assert!(
            Instant::now() < deadline,
            "the state never became {state}: {body}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn notice_texts(chat: &ChatStore) -> Vec<String> {
    let filter = ConversationFilter {
        q: None,
        section_id: None,
        pinned: false,
        archived: false,
        attention: false,
        working: false,
        cursor: None,
        limit: 100,
    };
    chat.list_conversations(filter)
        .expect("list conversations")
        .conversations
        .into_iter()
        .filter(|conversation| conversation.title == NOTICE_TITLE)
        .flat_map(|conversation| {
            chat.messages(&conversation.id, None, None, 100)
                .expect("messages")
                .messages
        })
        .map(|message| message.body_md)
        .collect()
}

/// Waits until a notice containing `text` is in the chat; fails after `WAIT`.
async fn wait_notice(chat: &ChatStore, text: &str) -> Vec<String> {
    let deadline = Instant::now() + WAIT;
    loop {
        let notices = notice_texts(chat);
        if notices.iter().any(|notice| notice.contains(text)) {
            return notices;
        }
        assert!(
            Instant::now() < deadline,
            "no notice with {text}: {notices:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_state_reads_as_claude_and_needs_only_read_state() {
    let host = TestHost::start("alfred-backend-read").await;
    let reader = paired(&host, "Lector", &[Permission::ReadState]).await;

    let (status, body) = read(&reader).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["backend"], "claude");
    assert_eq!(body["state"], "claude");
    assert!(body["detail"].is_null(), "{body}");
    assert!(body["since"].is_u64(), "{body}");
    assert_eq!(body["voice_suspended"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn changing_the_engine_needs_approve_and_read_state_is_not_enough() {
    let host = TestHost::start("alfred-backend-approve").await;
    let reader = paired(&host, "Lector", &[Permission::ReadState]).await;
    let agent = paired(
        &host,
        "Agente",
        &[Permission::ReadState, Permission::Respond],
    )
    .await;

    for client in [&reader, &agent] {
        let (status, body) = ask(client, "local").await;
        assert_eq!(status, 403, "{body}");
        assert_eq!(body["detail"], "forbidden");
    }
    assert_eq!(host.alfred_engine.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_engine_is_a_400() {
    let host = TestHost::start("alfred-backend-400").await;
    let owner = paired(
        &host,
        "Dueño",
        &[Permission::ReadState, Permission::Approve],
    )
    .await;

    for body in [
        "{\"backend\":\"gpt\"}",
        "{\"backend\":5}",
        "{}",
        "{\"backend\":\"Local\"}",
    ] {
        let (status, text) = owner
            .request("POST", PATH, Some(body.as_bytes().to_vec()))
            .await
            .expect("post");
        assert_eq!(status, 400, "{body} -> {text}");
    }
    assert_eq!(host.alfred_engine.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn local_answers_at_once_and_is_one_change_while_strata_loads() {
    let host = TestHost::start("alfred-backend-local").await;
    let owner = paired(
        &host,
        "Dueño",
        &[Permission::ReadState, Permission::Approve],
    )
    .await;
    host.alfred_engine.hold.store(true, Ordering::SeqCst);

    // The answer comes before the model is up: 202, and the switch says it is starting.
    let (status, body) = ask(&owner, "local").await;
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["state"], "starting");
    assert_eq!(
        body["backend"], "claude",
        "the marker waits for the model: {body}"
    );
    assert_eq!(body["voice_suspended"], true);

    // Once the start is under way and the model is still loading, the marker is not set yet.
    let deadline = Instant::now() + WAIT;
    while !host.alfred_engine.entered.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "the start never began");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (_, loading) = read(&owner).await;
    assert_eq!(loading["backend"], "claude", "{loading}");
    assert!(
        !alfred_backend::is_on(&host.data_dir),
        "no marker before the model answers"
    );

    // The same request again is the same change, not a second one; the other way is refused.
    let (status, _) = ask(&owner, "local").await;
    assert_eq!(status, 202);
    let (status, refused) = ask(&owner, "claude").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["detail"]["code"], "backend_busy");
    assert_eq!(host.alfred_engine.starts.load(Ordering::SeqCst), 1);

    host.alfred_engine.hold.store(false, Ordering::SeqCst);
    let body = wait_for(&owner, "local").await;
    assert_eq!(body["backend"], "local");
    assert_eq!(body["voice_suspended"], true);
    assert!(
        alfred_backend::is_on(&host.data_dir),
        "the marker is set once it answers"
    );

    let (status, body) = ask(&owner, "local").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["state"], "local");
    assert_eq!(host.alfred_engine.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn claude_gives_the_voice_and_the_model_back() {
    let host = TestHost::start("alfred-backend-claude").await;
    let owner = paired(
        &host,
        "Dueño",
        &[Permission::ReadState, Permission::Approve],
    )
    .await;
    assert_eq!(ask(&owner, "local").await.0, 202);
    wait_for(&owner, "local").await;

    let (status, body) = ask(&owner, "claude").await;
    assert_eq!(status, 202, "{body}");
    let body = wait_for(&owner, "claude").await;
    assert_eq!(body["backend"], "claude");
    assert_eq!(body["voice_suspended"], false, "the voice is back: {body}");
    assert!(
        !local_mode::is_on(&host.data_dir),
        "the local-mode marker is gone"
    );
    assert!(!alfred_backend::is_on(&host.data_dir));
    assert!(
        !host.alfred_engine.ready.load(Ordering::SeqCst),
        "the model is stopped"
    );

    let (status, body) = ask(&owner, "claude").await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_start_keeps_claude_and_a_retry_starts_again() {
    let host = TestHost::start("alfred-backend-fail").await;
    let owner = paired(
        &host,
        "Dueño",
        &[Permission::ReadState, Permission::Approve],
    )
    .await;
    host.alfred_engine.missing.store(true, Ordering::SeqCst);

    assert_eq!(ask(&owner, "local").await.0, 202);
    let body = wait_for(&owner, "failed").await;
    assert_eq!(
        body["backend"], "claude",
        "the engine is still Claude: {body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|text| text.contains("no encuentro Strata")),
        "{body}"
    );
    assert_eq!(body["voice_suspended"], false);
    assert!(
        !alfred_backend::is_on(&host.data_dir),
        "no half-written marker"
    );
    assert!(
        !local_mode::is_on(&host.data_dir),
        "the voice is not left down"
    );
    let notices = wait_notice(&host.chat, "No pude arrancar el modelo local").await;
    let failures = notices
        .iter()
        .filter(|notice| notice.starts_with("No pude arrancar el modelo local"))
        .count();
    assert_eq!(failures, 1, "one notice per failed change: {notices:?}");

    // A new request after `failed` tries again, and the failure is cleared.
    host.alfred_engine.missing.store(false, Ordering::SeqCst);
    assert_eq!(ask(&owner, "local").await.0, 202);
    let body = wait_for(&owner, "local").await;
    assert_eq!(body["detail"], Value::Null, "{body}");
    assert_eq!(host.alfred_engine.starts.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_state_is_read_from_the_markers_after_a_restart() {
    let host = TestHost::start("alfred-backend-restart").await;
    let engine = Arc::new(FakeEngine::default());
    let notices = Arc::new(PairingNotice::new(&host.data_dir, Arc::clone(&host.chat)));
    let restarted = Switch::new(
        host.data_dir.clone(),
        notices,
        Arc::clone(&engine) as Arc<dyn Engine>,
    );

    // The model is up and the markers say so: local, with the voice down.
    engine.ready.store(true, Ordering::SeqCst);
    alfred_backend::set(&host.data_dir).expect("set the alfred marker");
    local_mode::set(&host.data_dir).expect("set the local-mode marker");
    let status = restarted.status().await.to_json();
    assert_eq!(status["state"], "local", "{status}");
    assert_eq!(status["backend"], "local");
    assert_eq!(status["voice_suspended"], true);

    // Nothing on disk and no model: Claude, and the voice is back.
    engine.ready.store(false, Ordering::SeqCst);
    alfred_backend::clear(&host.data_dir).expect("clear the alfred marker");
    local_mode::clear(&host.data_dir).expect("clear the local-mode marker");
    let status = restarted.status().await.to_json();
    assert_eq!(status["state"], "claude", "{status}");
    assert_eq!(status["voice_suspended"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ready_cli_engine_is_listed_and_never_selected() {
    let host = TestHost::start("alfred-backend-engines").await;
    let engine = Arc::new(FakeEngine::default());
    let notices = Arc::new(PairingNotice::new(&host.data_dir, Arc::clone(&host.chat)));
    let switch = Switch::new(
        host.data_dir.clone(),
        notices,
        Arc::clone(&engine) as Arc<dyn Engine>,
    )
    .with_engines(vec![EngineListing {
        name: "opencode".to_owned(),
        version: "2.2.6".to_owned(),
    }]);

    let status = switch.status().await.to_json();
    assert_eq!(
        status["engines"],
        serde_json::json!([{ "name": "opencode", "version": "2.2.6" }])
    );
    assert_eq!(status["backend"], "claude");

    // Listing is not switching: a request for an engine other than local or claude is refused.
    assert!(switch.request("opencode").await.is_err());
}
