//! The alert rules over REST (F5): a device with `RESPOND` keeps its rules and the mode; a
//! device without it is refused; a refused body names its field; an identical rule is a
//! `409`, a missing rule a `404`; and `/v1/state` shows the mode and the count, never a rule.

mod harness;

use excubia_auth::Permission;
use excubia_test_support::SignedClient;
use harness::TestHost;
use serde_json::{json, Value};

/// One signed request, answered as the status and the parsed body.
async fn call(
    client: &SignedClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let bytes = body.map(|value| serde_json::to_vec(&value).expect("json"));
    let (status, text) = client.request(method, path, bytes).await.expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn reminder_rule() -> Value {
    json!({ "trigger": "calendar_reminder", "action": "notify" })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_with_respond_keeps_its_rules_and_the_mode() {
    let host = TestHost::start("alerts-rest-keep").await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;

    let (status, created) = call(&writer, "POST", "/v1/alerts/rules", Some(reminder_rule())).await;
    assert_eq!(status, 201, "{created}");
    let id = created["rule"]["id"].as_str().expect("rule id").to_string();
    assert_eq!(created["rule"]["created_by"], "user");
    assert_eq!(created["rule"]["enabled"], true);

    let (status, again) = call(&writer, "POST", "/v1/alerts/rules", Some(reminder_rule())).await;
    assert_eq!(status, 409, "{again}");
    assert_eq!(again["detail"], "rule_exists");

    let (status, listed) = call(&writer, "GET", "/v1/alerts/rules", None).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["rules"].as_array().map(Vec::len), Some(1));

    let (status, set) = call(
        &writer,
        "POST",
        "/v1/alerts/mode",
        Some(json!({ "mode": "rules_only" })),
    )
    .await;
    assert_eq!(status, 200, "{set}");
    let (status, mode) = call(&writer, "GET", "/v1/alerts/mode", None).await;
    assert_eq!(status, 200, "{mode}");
    assert_eq!(mode["mode"], "rules_only");

    let (status, updated) = call(
        &writer,
        "POST",
        &format!("/v1/alerts/rules/{id}/update"),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["rule"]["enabled"], false);

    let (status, deleted) = call(
        &writer,
        "POST",
        &format!("/v1/alerts/rules/{id}/delete"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    let (status, gone) = call(
        &writer,
        "POST",
        &format!("/v1/alerts/rules/{id}/delete"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{gone}");
    assert_eq!(gone["detail"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_body_names_its_field() {
    let host = TestHost::start("alerts-rest-fields").await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;

    // A pane rule needs a status; a reminder rule takes none.
    let (status, body) = call(
        &writer,
        "POST",
        "/v1/alerts/rules",
        Some(json!({ "trigger": "pane_status", "action": "notify" })),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "invalid_field");
    assert_eq!(body["detail"]["field"], "status");

    let (status, body) = call(
        &writer,
        "POST",
        "/v1/alerts/rules",
        Some(json!({ "trigger": "calendar_reminder", "action": "notify", "extra": 1 })),
    )
    .await;
    assert_eq!(status, 422, "{body}");

    let (status, body) = call(
        &writer,
        "POST",
        "/v1/alerts/mode",
        Some(json!({ "mode": "loud" })),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["field"], "mode");

    let (status, body) = call(
        &writer,
        "POST",
        "/v1/alerts/rules/ar_missing/update",
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_respond_cannot_change_rules_but_can_read_them() {
    let host = TestHost::start("alerts-rest-permission").await;
    let reader = host.paired_with("reader", &[]).await;

    let (status, _) = call(&reader, "POST", "/v1/alerts/rules", Some(reminder_rule())).await;
    assert_eq!(status, 403);
    let (status, _) = call(
        &reader,
        "POST",
        "/v1/alerts/mode",
        Some(json!({ "mode": "rules_only" })),
    )
    .await;
    assert_eq!(status, 403);

    let (status, listed) = call(&reader, "GET", "/v1/alerts/rules", None).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["rules"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn state_shows_the_mode_and_the_count_but_no_rule() {
    let host = TestHost::start("alerts-rest-state").await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (status, _) = call(&writer, "POST", "/v1/alerts/rules", Some(reminder_rule())).await;
    assert_eq!(status, 201);

    let (status, state) = call(&writer, "GET", "/v1/state", None).await;
    assert_eq!(status, 200, "{state}");
    assert_eq!(state["alert_rules"]["mode"], "all");
    assert_eq!(state["alert_rules"]["enabled_count"], 1);
    assert!(
        !state.to_string().contains("calendar_reminder"),
        "the state names a rule: {state}"
    );
}
