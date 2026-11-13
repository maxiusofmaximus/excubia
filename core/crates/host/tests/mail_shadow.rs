//! The shadow mode of `mail.triage` (F1): the host asks the triage sidecar about each
//! message it read and writes one line per answer. Alfred's reply is the same with or
//! without the sidecar, and the log never carries the mail itself.

mod mail_shadow_support;
mod mail_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use excubia_chat::{ChatStore, SeatActor};
use excubia_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use excubia_host::mail_shadow::{Shadow, ShadowLog, SHADOW_FILE};
use excubia_host::mail_sidecar::{Sidecar, SidecarEndpoint};
use excubia_host::mail_tools::{open_ledger, MailTools};
use excubia_mail::auth::Tenant;
use excubia_mail::{Account, AccountStore, FileKey, HttpResponse, HttpTransport, Secret, Tokens};
use excubia_seat::{InboxFolder, MailCall, TriageQuery};
use excubia_test_dir::TestDir;
use mail_shadow_support::{closed_port, serve, Reply};
use mail_support::{ok, token_answer, Scripted, ACCESS, REFRESH};
use serde_json::{json, Value};

const LIMIT: u64 = 5 * 1024 * 1024;
const SUBJECT: &str = "Factura confidencial";
const SENDER_NAME: &str = "Ana Ruiz";
const SENDER: &str = "ana@x.com";
const BODY: &str = "Cuerpo secreto del correo";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

/// The mail tools over a scripted Microsoft, with one account signed in.
fn tools(dir: &TestDir, answers: Vec<HttpResponse>) -> MailTools {
    let data = dir.join("data");
    let key = FileKey::new(dir.join("key"));
    let store = Arc::new(AccountStore::open(data.join("mail/accounts"), &key).expect("store"));
    let transport: Arc<dyn HttpTransport> = Arc::new(Scripted::new(answers));
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let client_id = Some("app".to_owned());
    let sends = MailSends::new(
        Arc::clone(&transport),
        Arc::clone(&store),
        client_id.clone(),
        open_ledger(&data).expect("ledger"),
        &data.join("mail/accounts"),
        Arc::clone(&chat),
        SendPolicy {
            per_hour: 10,
            window: DEFAULT_WINDOW,
        },
    );
    let tools = MailTools::new(
        transport,
        Arc::clone(&store),
        client_id,
        &data,
        chat,
        &dir.join("workspaces"),
        sends,
    )
    .expect("mail tools");
    let tokens = Tokens::new(ACCESS, Some(Secret::new(REFRESH)), 3600);
    let account = Account::new("trabajo", Tenant::Organizations, "ana@example.com", tokens);
    store.save(&account).expect("save");
    tools
}

/// One triage's worth of answers: the token, the listing, then each message and its
/// (empty) attachment list.
fn answers(ids: &[&str]) -> Vec<HttpResponse> {
    let items: Vec<Value> = ids.iter().copied().map(item).collect();
    let mut out = vec![token_answer(), ok(json!({ "value": items }))];
    for id in ids {
        out.push(ok(message(id)));
        out.push(ok(json!({ "value": [] })));
    }
    out
}

fn item(id: &str) -> Value {
    json!({
        "id": id,
        "subject": SUBJECT,
        "from": {"emailAddress": {"name": SENDER_NAME, "address": SENDER}},
        "receivedDateTime": "2026-10-30T07:00:00Z",
        "isRead": false,
        "hasAttachments": false,
        "bodyPreview": BODY,
    })
}

fn message(id: &str) -> Value {
    json!({
        "id": id,
        "subject": SUBJECT,
        "from": {"emailAddress": {"name": SENDER_NAME, "address": SENDER}},
        "sender": {"emailAddress": {"name": SENDER_NAME, "address": SENDER}},
        "replyTo": [],
        "toRecipients": [],
        "ccRecipients": [],
        "receivedDateTime": "2026-10-30T07:00:00Z",
        "internetMessageId": "<1@x.com>",
        "hasAttachments": false,
        "body": {"contentType": "text", "content": BODY},
        "internetMessageHeaders": [],
    })
}

/// Laya's answer, as the sidecar sends it.
fn answer() -> Value {
    json!({
        "model": "laya:typed-decisions",
        "questions": "mail_triage@p1",
        "answers": {
            "kind": {"personal": 0.7, "marketing": 0.3},
            "phish": {"sí": 0.1, "no": 0.9},
        },
        "latency_ms": 80,
    })
}

fn shadow(port: u16, deadline: Duration, dir: &TestDir) -> Arc<Shadow> {
    let endpoint = SidecarEndpoint::parse(&format!("http://127.0.0.1:{port}")).expect("endpoint");
    let log = ShadowLog::new(log_path(dir), LIMIT);
    Shadow::new(Sidecar::new(endpoint, deadline), log)
}

fn log_path(dir: &TestDir) -> PathBuf {
    dir.join("data").join(SHADOW_FILE)
}

async fn triage(tools: &MailTools) -> Value {
    let call = MailCall::Triage(TriageQuery {
        account: "trabajo".into(),
        folder: InboxFolder::Inbox,
        limit: 10,
        unread_only: false,
    });
    tools.call(&actor(), call).await.expect("triage").result
}

/// The lines of the log, once it holds `count` of them. The pass runs in the background.
async fn lines(path: &Path, count: usize) -> Vec<Value> {
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(path) {
            if text.lines().count() >= count {
                return text
                    .lines()
                    .map(|line| serde_json::from_str(line).expect("a json line"))
                    .collect();
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the shadow log never reached {count} lines");
}

#[tokio::test]
async fn a_healthy_sidecar_gets_one_line_per_message_with_its_probabilities() {
    let dir = TestDir::new("mail-shadow-healthy");
    let sidecar = serve(Reply::Json(answer())).await;
    let tools = tools(&dir, answers(&["m1", "m2"])).with_shadow(shadow(
        sidecar.port,
        Duration::from_secs(5),
        &dir,
    ));

    triage(&tools).await;

    let written = lines(&log_path(&dir), 2).await;
    assert_eq!(written[0]["status"], "ok");
    assert_eq!(written[0]["message_id"], "m1");
    assert_eq!(written[1]["message_id"], "m2");
    assert_eq!(written[0]["account"], "trabajo");
    assert_eq!(written[0]["folder"], "inbox");
    assert_eq!(written[0]["level"], "unknown");
    assert!(written[0]["ts"]
        .as_str()
        .is_some_and(|ts| ts.ends_with('Z')));
    assert_eq!(written[0]["sidecar"]["model"], "laya:typed-decisions");
    assert_eq!(written[0]["sidecar"]["answers"]["phish"]["sí"], 0.1);
    assert_eq!(written[0]["sidecar"]["answers"]["kind"]["personal"], 0.7);

    let bodies = sidecar.bodies.lock().expect("bodies");
    assert_eq!(bodies.len(), 2);
    let ficha: Value = serde_json::from_str(&bodies[0]).expect("the request is json");
    assert_eq!(ficha["state"]["subject"], SUBJECT);
    assert_eq!(ficha["state"]["preview"], BODY);
    assert_eq!(ficha["state"]["link_count"], 0);
    assert!(ficha["state"]["contact_known"].is_null());
    assert!(ficha["state"]["deterministic"]["signals"].is_array());
}

#[tokio::test]
async fn a_sidecar_that_is_down_slow_or_wrong_leaves_its_status_and_alfreds_reply_unchanged() {
    let slow = serve(Reply::Slow(Duration::from_secs(3), answer())).await;
    let garbage = serve(Reply::Raw(200, "basura")).await;
    let cases = [
        // Windows takes about 2 s to refuse a closed loopback port: the deadline must outlast that.
        (closed_port().await, Duration::from_secs(5), "unavailable"),
        (slow.port, Duration::from_millis(200), "timeout"),
        (garbage.port, Duration::from_secs(1), "bad_response"),
    ];
    for (index, (port, deadline, status)) in cases.into_iter().enumerate() {
        let plain_dir = TestDir::new(&format!("mail-shadow-plain-{index}"));
        let plain = triage(&tools(&plain_dir, answers(&["m1"]))).await;

        let dir = TestDir::new(&format!("mail-shadow-failure-{index}"));
        let with = tools(&dir, answers(&["m1"])).with_shadow(shadow(port, deadline, &dir));
        let shadowed = triage(&with).await;

        let written = lines(&log_path(&dir), 1).await;
        assert_eq!(written[0]["status"], status);
        assert!(written[0]["sidecar"].is_null(), "{status}");
        assert_eq!(shadowed, plain, "{status}");
    }
}

#[tokio::test]
async fn without_the_option_no_file_is_written() {
    let dir = TestDir::new("mail-shadow-off");
    triage(&tools(&dir, answers(&["m1"]))).await;
    assert!(!log_path(&dir).exists());
}

#[tokio::test]
async fn the_log_never_holds_the_subject_the_sender_or_the_body() {
    let dir = TestDir::new("mail-shadow-private");
    let sidecar = serve(Reply::Json(answer())).await;
    let tools = tools(&dir, answers(&["m1"])).with_shadow(shadow(
        sidecar.port,
        Duration::from_secs(5),
        &dir,
    ));
    triage(&tools).await;
    lines(&log_path(&dir), 1).await;

    let text = std::fs::read_to_string(log_path(&dir)).expect("log");
    for secret in [SUBJECT, SENDER_NAME, SENDER, BODY] {
        assert!(!text.contains(secret), "{secret} leaked into the log");
    }
}

#[tokio::test]
async fn a_slow_sidecar_does_not_hold_the_reply() {
    let slow = serve(Reply::Slow(Duration::from_secs(4), answer())).await;
    let dir = TestDir::new("mail-shadow-slow");
    let tools =
        tools(&dir, answers(&["m1"])).with_shadow(shadow(slow.port, Duration::from_secs(10), &dir));

    let started = Instant::now();
    triage(&tools).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the reply waited for the sidecar"
    );
}

#[tokio::test]
async fn a_batch_that_finds_a_pass_running_is_dropped_and_counted() {
    let slow = serve(Reply::Slow(Duration::from_millis(300), answer())).await;
    let dir = TestDir::new("mail-shadow-busy");
    let shadow = shadow(slow.port, Duration::from_secs(5), &dir);
    let script = [answers(&["m1"]), answers(&["m1"])].concat();
    let tools = tools(&dir, script).with_shadow(Arc::clone(&shadow));

    triage(&tools).await;
    triage(&tools).await;

    assert_eq!(shadow.skipped(), 1);
    lines(&log_path(&dir), 1).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(lines(&log_path(&dir), 1).await.len(), 1);
}

#[test]
fn the_log_moves_to_dot_one_when_it_reaches_its_limit() {
    let dir = TestDir::new("mail-shadow-rotate");
    let log = ShadowLog::new(dir.join(SHADOW_FILE), 120);
    for number in 0..4 {
        log.append(&json!({"n": number, "pad": "x".repeat(40)}))
            .expect("append");
    }

    let rotated = std::fs::read_to_string(dir.join("mail-shadow.jsonl.1")).expect("rotated");
    let current = std::fs::read_to_string(dir.join(SHADOW_FILE)).expect("current");
    assert_eq!(rotated.lines().count(), 3);
    assert_eq!(current.lines().count(), 1);
    assert!(current.contains("\"n\":3"), "{current}");
}
