mod common;

use std::sync::Mutex;

use common::{spawn_reply, Captured};
use herdr_desktop_core::chat::{export_file_name, ChatFailure, ICS_MAX_BYTES};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;

const JSON: &str = "Content-Type: application/json\r\n";
const ICS_FILE: &str = "Content-Type: text/calendar; charset=utf-8\r\nContent-Disposition: attachment; filename=\"excubia.ics\"\r\n";
const EXPORTED: &[u8] = b"BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n";
const IMPORTED: &str = r#"{"ok":true,"imported":2,"skipped_existing":1,"skipped_unsupported":1,"details":[{"index":1,"reason":"exists"},{"index":2,"reason":"unsupported_recurrence"}]}"#;

fn client(base_url: &str) -> GatewayClient {
    GatewayClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

/// The request the fake gateway saw. It is stored before the answer goes out.
fn seen(captured: &Mutex<Option<Captured>>) -> Captured {
    captured
        .lock()
        .unwrap()
        .take()
        .expect("el gateway no recibió la petición")
}

async fn import_refusal(status: u16, body: &'static str) -> ChatFailure {
    let (base_url, _) = spawn_reply(status, JSON, body.as_bytes()).await;
    client(&base_url)
        .calendar_import_ics(b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n")
        .await
        .unwrap_err()
}

async fn export_refusal(status: u16, body: &'static str) -> ChatFailure {
    let (base_url, _) = spawn_reply(status, JSON, body.as_bytes()).await;
    client(&base_url)
        .calendar_export_ics(Some(("2026-11-01", "2026-12-01")))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn an_import_sends_the_file_as_it_is_and_reads_the_counts() {
    let (base_url, captured) = spawn_reply(200, JSON, IMPORTED.as_bytes()).await;
    let file = EXPORTED.to_vec();

    let outcome = client(&base_url).calendar_import_ics(&file).await.unwrap();

    assert_eq!(
        (
            outcome.imported,
            outcome.skipped_existing,
            outcome.skipped_unsupported
        ),
        (2, 1, 1)
    );
    assert_eq!(outcome.details[1].index, 2);
    assert_eq!(outcome.details[1].reason, "unsupported_recurrence");
    let request = seen(&captured);
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v1/calendar/import");
    assert!(request
        .headers
        .to_ascii_lowercase()
        .contains("content-type: text/calendar"));
    assert!(request
        .headers
        .to_ascii_lowercase()
        .contains("x-hn-signature:"));
    assert_eq!(request.body, file);
}

#[tokio::test]
async fn a_file_of_exactly_the_limit_is_sent() {
    let (base_url, captured) = spawn_reply(200, JSON, IMPORTED.as_bytes()).await;

    client(&base_url)
        .calendar_import_ics(&vec![b'x'; ICS_MAX_BYTES])
        .await
        .unwrap();

    assert_eq!(seen(&captured).body.len(), ICS_MAX_BYTES);
}

#[tokio::test]
async fn a_file_over_the_limit_is_refused_before_anything_is_sent() {
    // Nothing listens on port 9: a request that left would fail as offline instead.
    let failure = client("http://127.0.0.1:9")
        .calendar_import_ics(&vec![b'x'; ICS_MAX_BYTES + 1])
        .await
        .unwrap_err();

    assert_eq!(
        failure,
        ChatFailure::FileRefused {
            code: "too_large".to_string()
        }
    );
}

#[tokio::test]
async fn an_export_asks_for_the_window_and_keeps_the_host_name() {
    let (base_url, captured) = spawn_reply(200, ICS_FILE, EXPORTED).await;

    let export = client(&base_url)
        .calendar_export_ics(Some(("2026-11-01", "2026-12-01")))
        .await
        .unwrap();

    assert_eq!(export.file_name, "excubia.ics");
    assert_eq!(export.bytes, EXPORTED);
    let request = seen(&captured);
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        "/v1/calendar/export.ics?from=2026-11-01&to=2026-12-01"
    );
}

#[tokio::test]
async fn an_export_without_a_window_asks_for_every_event() {
    let (base_url, captured) = spawn_reply(200, ICS_FILE, EXPORTED).await;

    client(&base_url).calendar_export_ics(None).await.unwrap();

    assert_eq!(seen(&captured).target, "/v1/calendar/export.ics");
}

#[tokio::test]
async fn a_window_with_too_many_events_names_its_code_for_the_screen() {
    let failure = export_refusal(
        422,
        r#"{"detail":{"code":"too_many_events","message":"más de 2000"}}"#,
    )
    .await;

    assert_eq!(
        failure,
        ChatFailure::AgendaRefused {
            code: "too_many_events".to_string(),
            field: None,
            message: "más de 2000".to_string(),
        }
    );
}

#[tokio::test]
async fn an_export_without_the_company_zone_names_timezone_unset() {
    let failure = export_refusal(
        409,
        r#"{"detail":{"code":"timezone_unset","message":"sin zona"}}"#,
    )
    .await;

    assert!(matches!(
        failure,
        ChatFailure::AgendaRefused { ref code, .. } if code == "timezone_unset"
    ));
}

#[tokio::test]
async fn a_window_that_ends_before_it_starts_names_the_field_to() {
    let failure = export_refusal(
        422,
        r#"{"detail":{"code":"invalid_field","field":"to","message":"antes"}}"#,
    )
    .await;

    assert!(matches!(
        failure,
        ChatFailure::Invalid { field: Some(ref field), .. } if field == "to"
    ));
}

#[tokio::test]
async fn a_file_that_is_not_a_calendar_names_invalid_ics() {
    let failure = import_refusal(
        400,
        r#"{"detail":{"code":"invalid_ics","message":"sin VCALENDAR"}}"#,
    )
    .await;

    assert!(matches!(
        failure,
        ChatFailure::AgendaRefused { ref code, .. } if code == "invalid_ics"
    ));
}

#[tokio::test]
async fn a_file_with_too_many_events_names_its_code_for_the_screen() {
    let failure = import_refusal(
        422,
        r#"{"detail":{"code":"too_many_events","message":"más de 500"}}"#,
    )
    .await;

    assert!(matches!(
        failure,
        ChatFailure::AgendaRefused { ref code, .. } if code == "too_many_events"
    ));
}

#[tokio::test]
async fn a_file_the_host_will_not_take_keeps_its_status() {
    let failure = import_refusal(413, r#"{"detail":"demasiado grande"}"#).await;

    assert!(matches!(failure, ChatFailure::Gateway { status: 413, .. }));
}

#[tokio::test]
async fn an_import_without_the_permission_is_forbidden() {
    let failure = import_refusal(403, r#"{"detail":"forbidden"}"#).await;

    assert!(matches!(failure, ChatFailure::Forbidden { .. }));
}

#[test]
fn the_save_name_is_the_host_name_when_it_is_a_plain_ics_file() {
    assert_eq!(
        export_file_name(Some("attachment; filename=\"excubia.ics\"")),
        "excubia.ics"
    );
    assert_eq!(
        export_file_name(Some("attachment; filename=Mi-Calendario_2026.ICS")),
        "Mi-Calendario_2026.ICS"
    );
}

#[test]
fn a_save_name_with_a_path_or_a_parent_or_another_type_falls_back() {
    let hostile = [
        "attachment; filename=\"../../Desktop/x.ics\"",
        "attachment; filename=\"a/b.ics\"",
        "attachment; filename=\"a\\b.ics\"",
        "attachment; filename=\"C:x.ics\"",
        "attachment; filename=\"..ics\"",
        "attachment; filename=\".oculto.ics\"",
        "attachment; filename=\"calendario.exe\"",
        "attachment; filename=\"\"",
    ];
    for disposition in hostile {
        assert_eq!(
            export_file_name(Some(disposition)),
            "excubia.ics",
            "{disposition}"
        );
    }
    assert_eq!(export_file_name(None), "excubia.ics");
}
