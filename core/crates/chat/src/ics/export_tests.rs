use super::{export_calendar, read_calendar};
use crate::model_calendar::CalendarEvent;

fn event() -> CalendarEvent {
    CalendarEvent {
        id: "ev_1".to_string(),
        title: "Llamada".to_string(),
        start_utc: "2026-11-03T08:30:00.000Z".to_string(),
        end_utc: Some("2026-11-03T09:30:00.000Z".to_string()),
        all_day: false,
        recurrence: None,
        timezone: "Europe/Madrid".to_string(),
        location: None,
        notes: None,
        reminder_minutes: None,
        reminded_at: None,
        reminded_occurrence: None,
        uid: None,
        origin_kind: "device".to_string(),
        origin_id: "device-1".to_string(),
        conversation_id: None,
        created_at: "2026-10-29T10:00:00.000Z".to_string(),
        updated_at: "2026-10-29T10:00:00.000Z".to_string(),
    }
}

fn export(events: &[CalendarEvent]) -> String {
    export_calendar(events).expect("an export")
}

fn lines(text: &str) -> Vec<&str> {
    text.split("\r\n").collect()
}

#[test]
fn the_calendar_has_its_header_and_crlf_endings() {
    let text = export(&[event()]);
    assert!(text.starts_with(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Excubia//Calendario//ES\r\nCALSCALE:GREGORIAN\r\n"
    ));
    assert!(text.ends_with("END:VCALENDAR\r\n"));
    assert_eq!(text.matches('\n').count(), text.matches("\r\n").count());
}

#[test]
fn an_empty_company_exports_a_calendar_without_events() {
    let text = export(&[]);
    assert!(text.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(!text.contains("BEGIN:VEVENT"));
}

#[test]
fn a_timed_event_is_written_in_utc_with_its_uid_and_stamp() {
    let text = export(&[event()]);
    assert!(text.contains("\r\nBEGIN:VEVENT\r\nUID:ev_1@excubia\r\n"));
    assert!(text.contains("\r\nDTSTAMP:20261029T100000Z\r\n"));
    assert!(text.contains("\r\nDTSTART:20261103T083000Z\r\n"));
    assert!(text.contains("\r\nDTEND:20261103T093000Z\r\n"));
    assert!(text.contains("\r\nSUMMARY:Llamada\r\n"));
    assert!(text.contains("\r\nEND:VEVENT\r\n"));
}

#[test]
fn an_event_without_end_has_no_dtend() {
    let mut open = event();
    open.end_utc = None;
    assert!(!export(&[open]).contains("DTEND"));
}

#[test]
fn an_all_day_event_is_a_date_and_its_end_is_the_day_after_the_last() {
    let mut day = event();
    day.all_day = true;
    day.start_utc = "2026-11-02T23:00:00.000Z".to_string();
    day.end_utc = Some("2026-11-04T23:00:00.000Z".to_string());
    let text = export(&[day]);
    assert!(text.contains("\r\nDTSTART;VALUE=DATE:20261103\r\n"));
    assert!(text.contains("\r\nDTEND;VALUE=DATE:20261105\r\n"));
}

#[test]
fn a_series_keeps_its_rule_and_is_written_once() {
    let mut series = event();
    series.recurrence = Some("FREQ=WEEKLY;INTERVAL=1;COUNT=3".to_string());
    let text = export(&[series]);
    assert!(text.contains("\r\nRRULE:FREQ=WEEKLY;INTERVAL=1;COUNT=3\r\n"));
    assert_eq!(text.matches("BEGIN:VEVENT").count(), 1);
}

#[test]
fn a_reminder_is_a_valarm_with_a_minutes_trigger() {
    let mut reminded = event();
    reminded.reminder_minutes = Some(15);
    let text = export(&[reminded]);
    assert!(text.contains(
        "\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nDESCRIPTION:Llamada\r\nEND:VALARM\r\n"
    ));
}

#[test]
fn text_is_escaped_and_location_and_notes_appear_only_when_kept() {
    let mut texts = event();
    texts.title = "Cena, con Ana; ñ 🎉".to_string();
    texts.location = Some("Sala 3".to_string());
    texts.notes = Some("uno\ndos".to_string());
    let text = export(&[texts]);
    assert!(text.contains("\r\nSUMMARY:Cena\\, con Ana\\; ñ 🎉\r\n"));
    assert!(text.contains("\r\nLOCATION:Sala 3\r\n"));
    assert!(text.contains("\r\nDESCRIPTION:uno\\ndos\r\n"));
    let bare = export(&[event()]);
    assert!(!bare.contains("LOCATION") && !bare.contains("DESCRIPTION"));
}

#[test]
fn a_kept_uid_is_written_as_it_was() {
    let mut imported = event();
    imported.uid = Some("abc@example.com".to_string());
    assert!(export(&[imported]).contains("\r\nUID:abc@example.com\r\n"));
}

#[test]
fn every_physical_line_is_at_most_75_octets() {
    let mut long = event();
    long.title = "á".repeat(120);
    long.notes = Some("🎉".repeat(1_000));
    let text = export(&[long]);
    for line in lines(&text).iter().filter(|line| !line.is_empty()) {
        assert!(line.len() <= 75, "{line:?}");
    }
}

#[test]
fn an_export_reads_back_with_the_same_uid_title_and_instants() {
    let mut series = event();
    series.recurrence = Some("FREQ=DAILY;COUNT=2".to_string());
    series.reminder_minutes = Some(10);
    let text = export(&[series]);
    let drafts = read_calendar(text.as_bytes()).expect("a calendar");
    let draft = drafts[0].outcome.as_ref().expect("an event");
    assert_eq!(draft.uid, "ev_1@excubia");
    assert_eq!(draft.event.title, "Llamada");
    assert_eq!(draft.event.reminder_minutes, Some(10));
    assert_eq!(
        draft.event.recurrence.as_ref().map(ToString::to_string),
        Some("FREQ=DAILY;INTERVAL=1;COUNT=2".to_string())
    );
}
