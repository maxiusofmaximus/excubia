//! The `.ics` exchange over REST (CALENDARIO-F4c): the company's events come out as a file and
//! go back in; an import adds only what the company does not keep yet, and a bad file is
//! refused as a whole. Reading the export needs `READ_STATE`, importing needs `RESPOND`.

mod harness;

use harness::TestHost;
use excubia_core_auth::Permission;
use excubia_core_test_support::SignedClient;
use serde_json::{json, Value};

const EXPORT: &str = "/v1/calendar/export.ics?from=2026-11-01&to=2026-11-30";
const IMPORT: &str = "/v1/calendar/import";
const FIXTURE: &[u8] = include_bytes!("../../chat/src/ics/testdata/google-outlook.ics");

fn code_of(body: &Value) -> &str {
    body["detail"]["code"].as_str().expect("code")
}

async fn create(client: &SignedClient, title: &str, start: &str) -> (u16, Value) {
    let bytes = serde_json::to_vec(&json!({ "title": title, "start": start })).expect("json");
    let (status, text) = client
        .request("POST", "/v1/calendar/events", Some(bytes))
        .await
        .expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// One import of a file, answered as the status and the parsed report.
async fn import(client: &SignedClient, file: Vec<u8>) -> (u16, Value) {
    let (status, _, bytes) = client
        .request_calendar("POST", IMPORT, file)
        .await
        .expect("request");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_company_export_imports_back_as_existing_and_a_mail_file_adds_its_events() {
    let host = TestHost::start("calendar-ics-round-trip").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    for (title, start) in [
        ("Llamada con Ana", "2026-11-03T09:30"),
        ("Revisión", "2026-11-04T10:00"),
    ] {
        let (status, body) = create(&writer, title, start).await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, headers, file) = writer
        .request_raw("GET", EXPORT, None)
        .await
        .expect("request");
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "text/calendar; charset=utf-8");
    assert_eq!(
        headers["content-disposition"],
        "attachment; filename=\"excubia.ics\""
    );
    let text = String::from_utf8(file.clone()).expect("text");
    assert_eq!(text.matches("BEGIN:VEVENT").count(), 2);

    let (status, report) = import(&writer, file).await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["imported"], 0);
    assert_eq!(report["skipped_existing"], 2);

    let (status, report) = import(&writer, FIXTURE.to_vec()).await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["imported"], 3);
    assert_eq!(report["skipped_unsupported"], 2);

    let (_, again) = import(&writer, FIXTURE.to_vec()).await;
    assert_eq!(again["imported"], 0);
    assert_eq!(again["skipped_existing"], 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_garbage_file_is_refused_and_five_hundred_and_one_events_are_422() {
    let host = TestHost::start("calendar-ics-refused").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let writer = host.paired_with("writer", &[Permission::Respond]).await;

    let (status, refused) = import(&writer, b"esto no es un calendario".to_vec()).await;
    assert_eq!(status, 400, "{refused}");
    assert_eq!(code_of(&refused), "invalid_ics");

    let event = "BEGIN:VEVENT\r\nSUMMARY:Hola\r\nDTSTART:20261103T083000Z\r\nEND:VEVENT\r\n";
    let many = format!("BEGIN:VCALENDAR\r\n{}END:VCALENDAR\r\n", event.repeat(501));
    let (status, refused) = import(&writer, many.into_bytes()).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(code_of(&refused), "too_many_events");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_over_512_kib_is_413_before_it_is_read() {
    let host = TestHost::start("calendar-ics-too-large").await;
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let (status, _, _) = writer
        .request_calendar("POST", IMPORT, vec![b' '; 512 * 1024 + 1])
        .await
        .expect("request");
    assert_eq!(status, 413);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_respond_cannot_import_and_the_export_needs_a_zone() {
    let host = TestHost::start("calendar-ics-grants").await;
    let reader = host.paired_with("reader", &[]).await;

    let (status, refused) = import(&reader, FIXTURE.to_vec()).await;
    assert_eq!(status, 403, "{refused}");

    let (status, _, bytes) = reader
        .request_raw("GET", EXPORT, None)
        .await
        .expect("request");
    let refused: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code_of(&refused), "timezone_unset");

    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    let (status, _, _) = reader
        .request_raw("GET", EXPORT, None)
        .await
        .expect("request");
    assert_eq!(status, 200);

    let (status, _, bytes) = reader
        .request_raw(
            "GET",
            "/v1/calendar/export.ics?from=2026-11-08&to=2026-11-01",
            None,
        )
        .await
        .expect("request");
    let refused: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(status, 422, "{refused}");
}
