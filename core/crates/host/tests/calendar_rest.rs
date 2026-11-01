//! The calendar over REST (CALENDARIO-F4): a paired device with `RESPOND` keeps and moves
//! its events in the company's zone; a delete and the zone itself need `APPROVE`; a
//! company with no zone keeps no event; and a seat token, which is not a device, gets 401.

mod harness;

use harness::{TestHost, TOOL_TOKEN};
use hn_core_auth::Permission;
use hn_core_test_support::SignedClient;
use serde_json::{json, Value};

const WINDOW: &str = "/v1/calendar/events?from=2026-11-01&to=2026-11-08";

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

fn code_of(body: &Value) -> &str {
    body["detail"]["code"].as_str().expect("code")
}

fn id_of(body: &Value) -> String {
    body["event"]["id"].as_str().expect("event id").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_is_kept_in_the_company_zone_and_listed_in_its_window() {
    let host = TestHost::start("calendar-rest-create").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let writer = host.paired_with("writer", &[Permission::Respond]).await;

    let (status, created) = call(
        &writer,
        "POST",
        "/v1/calendar/events",
        Some(json!({
            "title": "Llamada con Ana",
            "start": "2026-11-03T09:30",
            "reminder_minutes": 15
        })),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["event"]["start_utc"], "2026-11-03T08:30:00.000Z");

    let (status, listed) = call(&writer, "GET", WINDOW, None).await;
    assert_eq!(status, 200, "{listed}");
    let listed_ids: Vec<String> = listed["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["id"].as_str().expect("id").to_string())
        .collect();
    assert_eq!(listed_ids, vec![id_of(&created)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_waits_for_the_company_zone_and_bad_windows_are_refused() {
    let host = TestHost::start("calendar-rest-zone").await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (status, refused) = call(
        &writer,
        "POST",
        "/v1/calendar/events",
        Some(json!({ "title": "Llamada", "start": "2026-11-03T09:30" })),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code_of(&refused), "timezone_unset");

    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let (status, refused) = call(
        &writer,
        "GET",
        "/v1/calendar/events?from=2026-11-08&to=2026-11-01",
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    for path in [
        "/v1/calendar/events?from=2026-11-01&to=2026-11-08&limit=0",
        "/v1/calendar/events?from=2026-11-01&to=2026-11-08&limit=101",
    ] {
        let (status, refused) = call(&writer, "GET", path, None).await;
        assert_eq!(status, 422, "{path}: {refused}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_approve_cannot_delete_or_set_the_zone() {
    let host = TestHost::start("calendar-rest-grants").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let reader = host.paired_with("reader", &[]).await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (_, created) = call(
        &writer,
        "POST",
        "/v1/calendar/events",
        Some(json!({ "title": "Dentista", "start": "2026-11-04T10:00" })),
    )
    .await;
    let id = id_of(&created);

    let (status, _) = call(
        &reader,
        "POST",
        "/v1/calendar/events",
        Some(json!({ "title": "x", "start": "2026-11-04T10:00" })),
    )
    .await;
    assert_eq!(status, 403);
    let (status, _) = call(&reader, "GET", WINDOW, None).await;
    assert_eq!(status, 200, "reading needs only the read grant");

    let (status, refused) = call(
        &writer,
        "POST",
        &format!("/v1/calendar/events/{id}/delete"),
        None,
    )
    .await;
    assert_eq!(status, 403, "a delete needs APPROVE: {refused}");
    let (status, refused) = call(
        &writer,
        "POST",
        "/v1/company/timezone",
        Some(json!({ "timezone": "Europe/Paris" })),
    )
    .await;
    assert_eq!(status, 403, "the zone needs APPROVE: {refused}");
    let (status, _) = call(&writer, "GET", &format!("/v1/calendar/events/{id}"), None).await;
    assert_eq!(status, 200, "the event survives the refused delete");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_app_moves_and_deletes_its_events_and_a_seat_token_gets_401() {
    let host = TestHost::start("calendar-rest-flow").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let owner = host
        .paired_with("owner", &[Permission::Respond, Permission::Approve])
        .await;
    let (status, zone) = call(&owner, "GET", "/v1/company/timezone", None).await;
    assert_eq!(status, 200, "{zone}");
    assert_eq!(zone["timezone"], "Europe/Madrid");
    let (status, refused) = call(
        &owner,
        "POST",
        "/v1/company/timezone",
        Some(json!({ "timezone": "Europe/Paris" })),
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code_of(&refused), "timezone_already_set");

    let (_, created) = call(
        &owner,
        "POST",
        "/v1/calendar/events",
        Some(json!({ "title": "Revisión", "start": "2026-11-05T12:00" })),
    )
    .await;
    let id = id_of(&created);

    let (status, moved) = call(
        &owner,
        "POST",
        &format!("/v1/calendar/events/{id}/update"),
        Some(json!({ "start": "2026-11-05T16:00" })),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["event"]["start_utc"], "2026-11-05T15:00:00.000Z");

    let (status, deleted) = call(
        &owner,
        "POST",
        &format!("/v1/calendar/events/{id}/delete"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    let (status, gone) = call(&owner, "GET", &format!("/v1/calendar/events/{id}"), None).await;
    assert_eq!(status, 404, "{gone}");
    assert_eq!(code_of(&gone), "not_found");

    let refused = reqwest::Client::new()
        .get(format!("{}{WINDOW}", host.base_url()))
        .header("X-HN-Tool-Token", TOOL_TOKEN)
        .send()
        .await
        .expect("read");
    assert_eq!(refused.status().as_u16(), 401);
}

/// A series is listed once per occurrence, a page that stops inside it resumes by its cursor,
/// and an empty recurrence makes the event a single one again.
#[tokio::test(flavor = "multi_thread")]
async fn a_series_is_listed_by_occurrence_and_paged_by_cursor() {
    let host = TestHost::start("calendar-rest-series").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (status, created) = call(
        &writer,
        "POST",
        "/v1/calendar/events",
        Some(json!({
            "title": "Clase",
            "start": "2026-11-02T09:00",
            "recurrence": "FREQ=DAILY;COUNT=3"
        })),
    )
    .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(
        created["event"]["recurrence"],
        "FREQ=DAILY;INTERVAL=1;COUNT=3"
    );
    let id = id_of(&created);

    let (status, page) = call(&writer, "GET", &format!("{WINDOW}&limit=2"), None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["truncated"], true);
    assert_eq!(
        page["events"][1]["occurrence_start"],
        "2026-11-03T08:00:00.000Z"
    );
    assert_eq!(page["events"][1]["recurring"], true);
    let next = page["next"].as_str().expect("next cursor").to_string();
    assert_eq!(next, format!("{id}@2026-11-03T08:00:00.000Z"));

    let (status, rest) = call(&writer, "GET", &format!("{WINDOW}&after={next}"), None).await;
    assert_eq!(status, 200, "{rest}");
    assert_eq!(rest["events"].as_array().expect("events").len(), 1);
    assert_eq!(
        rest["events"][0]["occurrence_start"],
        "2026-11-04T08:00:00.000Z"
    );
    assert_eq!(rest["truncated"], false);
    assert!(rest["next"].is_null());

    let (status, edited) = call(
        &writer,
        "POST",
        &format!("/v1/calendar/events/{id}/update"),
        Some(json!({ "recurrence": "" })),
    )
    .await;
    assert_eq!(status, 200, "{edited}");
    assert!(edited["event"]["recurrence"].is_null());
}

/// A rule this version does not expand is refused with its own code.
#[tokio::test(flavor = "multi_thread")]
async fn an_unsupported_rule_is_refused_with_its_code() {
    let host = TestHost::start("calendar-rest-rule").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (status, refused) = call(
        &writer,
        "POST",
        "/v1/calendar/events",
        Some(json!({
            "title": "Clase",
            "start": "2026-11-02T09:00",
            "recurrence": "FREQ=HOURLY"
        })),
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(code_of(&refused), "unsupported_recurrence");
}
