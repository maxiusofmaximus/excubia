mod common;

use std::sync::Mutex;

use common::{spawn_reply, Captured};
use herdr_desktop_core::chat::{
    AlertAction, AlertMode, AlertRuleDraft, AlertStatus, AlertTrigger, ChatFailure,
};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::HostClient;

const JSON: &str = "Content-Type: application/json\r\n";
const RULE: &str = r#"{"ok":true,"rule":{"id":"ar_01JABCD","trigger":"pane_status","agent":"Bob","status":"done","action":"notify","enabled":true,"created_by":"user","created_at":"2026-10-29T10:00:00Z"}}"#;

fn client(base_url: &str) -> HostClient {
    HostClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

/// The request the fake host saw. It is stored before the answer goes out.
fn seen(captured: &Mutex<Option<Captured>>) -> Captured {
    captured
        .lock()
        .unwrap()
        .take()
        .expect("el PC no recibió la petición")
}

fn bob_done_notify() -> AlertRuleDraft {
    AlertRuleDraft {
        trigger: AlertTrigger::PaneStatus,
        agent: Some("Bob".to_string()),
        status: Some(AlertStatus::Done),
        action: AlertAction::Notify,
    }
}

async fn refusal(status: u16, body: &'static str) -> ChatFailure {
    let (base_url, _) = spawn_reply(status, JSON, body.as_bytes()).await;
    client(&base_url)
        .alert_rule_set_enabled("ar_01JABCD", false)
        .await
        .unwrap_err()
}

#[tokio::test]
async fn a_rule_is_created_with_the_exact_body_and_signed() {
    let (base_url, captured) = spawn_reply(201, JSON, RULE.as_bytes()).await;

    let rule = client(&base_url)
        .alert_rule_create(&bob_done_notify())
        .await
        .unwrap();

    let sent = seen(&captured);
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.target, "/v1/alerts/rules");
    assert!(sent.headers.contains("x-hn-signature:"), "{}", sent.headers);
    assert_eq!(
        String::from_utf8(sent.body).unwrap(),
        r#"{"trigger":"pane_status","agent":"Bob","status":"done","action":"notify"}"#
    );
    assert_eq!(rule.id, "ar_01JABCD");
    assert_eq!(rule.status, Some(AlertStatus::Done));
}

#[tokio::test]
async fn a_reminder_has_no_status_and_any_agent_is_null() {
    let (base_url, captured) = spawn_reply(201, JSON, RULE.as_bytes()).await;
    let draft = AlertRuleDraft {
        trigger: AlertTrigger::CalendarReminder,
        agent: None,
        status: None,
        action: AlertAction::Mute,
    };

    client(&base_url).alert_rule_create(&draft).await.unwrap();

    assert_eq!(
        String::from_utf8(seen(&captured).body).unwrap(),
        r#"{"trigger":"calendar_reminder","agent":null,"action":"mute"}"#
    );
}

#[tokio::test]
async fn the_rules_are_listed_with_their_null_agent_and_status() {
    let body = r#"{"ok":true,"rules":[{"id":"ar_01JABCD","trigger":"calendar_reminder","agent":null,"status":null,"action":"notify","enabled":false,"created_by":"alfred","created_at":"2026-10-29T10:00:00Z"}]}"#;
    let (base_url, captured) = spawn_reply(200, JSON, body.as_bytes()).await;

    let rules = client(&base_url).alert_rules().await.unwrap();

    let sent = seen(&captured);
    assert_eq!(
        (sent.method.as_str(), sent.target.as_str()),
        ("GET", "/v1/alerts/rules")
    );
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].agent, None);
    assert_eq!(rules[0].status, None);
    assert!(!rules[0].enabled);
}

#[tokio::test]
async fn an_enabled_flag_is_sent_to_the_rule_it_names() {
    let (base_url, captured) = spawn_reply(200, JSON, RULE.as_bytes()).await;

    client(&base_url)
        .alert_rule_set_enabled("ar_01JABCD", false)
        .await
        .unwrap();

    let sent = seen(&captured);
    assert_eq!(sent.target, "/v1/alerts/rules/ar_01JABCD/update");
    assert_eq!(
        String::from_utf8(sent.body).unwrap(),
        r#"{"enabled":false}"#
    );
}

#[tokio::test]
async fn a_delete_posts_to_the_rule_route() {
    let (base_url, captured) = spawn_reply(200, JSON, br#"{"ok":true,"id":"ar_01JABCD"}"#).await;

    client(&base_url)
        .alert_rule_delete("ar_01JABCD")
        .await
        .unwrap();

    let sent = seen(&captured);
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.target, "/v1/alerts/rules/ar_01JABCD/delete");
}

#[tokio::test]
async fn an_id_that_is_not_a_rule_never_leaves_the_device() {
    let (base_url, captured) = spawn_reply(200, JSON, RULE.as_bytes()).await;

    let outcome = client(&base_url).alert_rule_delete("nt_01JABCD").await;

    assert!(matches!(outcome, Err(ChatFailure::Invalid { .. })));
    assert!(captured.lock().unwrap().is_none());
}

#[tokio::test]
async fn the_mode_is_read_and_set_by_its_word() {
    let (base_url, captured) = spawn_reply(200, JSON, br#"{"ok":true,"mode":"all"}"#).await;
    assert_eq!(
        client(&base_url).alert_mode().await.unwrap(),
        AlertMode::All
    );
    assert_eq!(seen(&captured).target, "/v1/alerts/mode");

    let (base_url, captured) = spawn_reply(200, JSON, br#"{"ok":true,"mode":"rules_only"}"#).await;
    let mode = client(&base_url)
        .alert_mode_set(AlertMode::RulesOnly)
        .await
        .unwrap();

    let sent = seen(&captured);
    assert_eq!(mode, AlertMode::RulesOnly);
    assert_eq!(
        String::from_utf8(sent.body).unwrap(),
        r#"{"mode":"rules_only"}"#
    );
}

#[tokio::test]
async fn a_duplicate_rule_says_so_in_spanish() {
    match refusal(409, r#"{"ok":false,"detail":"rule_exists"}"#).await {
        ChatFailure::AlertRefused { code, message } => {
            assert_eq!(code, "rule_exists");
            assert_eq!(message, "Ya existe una regla igual");
        }
        other => panic!("se esperaba AlertRefused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn a_full_list_says_the_limit_in_spanish() {
    match refusal(409, r#"{"ok":false,"detail":"rule_limit"}"#).await {
        ChatFailure::AlertRefused { code, message } => {
            assert_eq!(code, "rule_limit");
            assert_eq!(message, "Has llegado al máximo de 100 reglas");
        }
        other => panic!("se esperaba AlertRefused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_rule_is_a_rule_refusal_not_a_conversation() {
    match refusal(404, r#"{"ok":false,"detail":"not_found"}"#).await {
        ChatFailure::AlertRefused { code, message } => {
            assert_eq!(code, "not_found");
            assert_eq!(message, "Esa regla ya no existe.");
        }
        other => panic!("se esperaba AlertRefused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn an_invalid_field_keeps_its_name() {
    let failure = refusal(
        422,
        r#"{"ok":false,"detail":{"code":"invalid_field","field":"status"}}"#,
    )
    .await;

    assert!(matches!(
        failure,
        ChatFailure::Invalid { field: Some(ref field), .. } if field == "status"
    ));
}

#[tokio::test]
async fn a_read_without_the_grant_is_forbidden() {
    let (base_url, _) = spawn_reply(403, JSON, br#"{"ok":false,"detail":"forbidden"}"#).await;

    let failure = client(&base_url).alert_rules().await.unwrap_err();

    assert!(matches!(failure, ChatFailure::Forbidden { .. }));
}
