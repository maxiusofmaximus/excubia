use std::sync::Arc;

use excubia_test_dir::TestDir;

use super::calendar::EventOrigin;
use crate::error::CalendarError;
use crate::model_calendar::{EventPatch, NewEvent, Window, MAX_EVENTS};
use crate::store::ChatStore;
use crate::timezone::EventTime;
use crate::{ManualClock, NewConversation};

/// 2026-10-29T00:00:00.000Z, the instant every test starts from.
pub(super) const START_MS: i64 = 1_793_232_000_000;
pub(super) const MINUTE_MS: i64 = 60_000;

pub(super) fn store_at(now_ms: i64) -> (TestDir, ChatStore, Arc<ManualClock>) {
    let dir = TestDir::new("calendar-store");
    let store = ChatStore::open(&dir.join("chat.db")).expect("store");
    let clock = Arc::new(ManualClock::at(now_ms));
    store.set_clock(clock.clone());
    (dir, store, clock)
}

pub(super) fn madrid(store: &ChatStore) {
    store
        .set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
}

pub(super) fn timed(title: &str, local: &str) -> NewEvent {
    NewEvent {
        title: title.to_string(),
        start: EventTime::parse(local).expect("time"),
        end: None,
        all_day: false,
        location: None,
        notes: None,
        reminder_minutes: None,
        recurrence: None,
    }
}

pub(super) fn device(store: &ChatStore, new: NewEvent) -> crate::model_calendar::CalendarEvent {
    store
        .create_event(EventOrigin::Device("device-1"), new)
        .expect("event")
}

pub(super) fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn audit_rows(store: &ChatStore) -> i64 {
    let connection = store.lock();
    connection
        .query_row(
            "SELECT COUNT(*) FROM agenda_audit WHERE object_kind = 'calendar_event'",
            [],
            |row| row.get(0),
        )
        .expect("audit")
}

#[test]
fn a_company_without_a_zone_creates_nothing() {
    let (_dir, store, _clock) = store_at(START_MS);
    let error = store
        .create_event(
            EventOrigin::Device("device-1"),
            timed("Cita", "2026-11-03T09:30"),
        )
        .expect_err("no zone");
    assert!(matches!(error, CalendarError::TimezoneUnset));
    assert_eq!(store.company_timezone().expect("zone"), None);
    assert_eq!(audit_rows(&store), 0);
}

#[test]
fn only_iana_zones_are_kept() {
    let (_dir, store, _clock) = store_at(START_MS);
    let error = store
        .set_company_timezone("device-1", "Mars/Olympus")
        .expect_err("not a zone");
    assert!(matches!(error, CalendarError::InvalidZone));
    store
        .set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    assert_eq!(
        store.company_timezone().expect("zone").as_deref(),
        Some("Europe/Madrid")
    );
}

/// The zone is fixed once: a second name is refused and the first one stays.
#[test]
fn the_company_zone_is_fixed_once() {
    let (_dir, store, _clock) = store_at(START_MS);
    store
        .set_company_timezone("device-1", "Europe/Madrid")
        .expect("zone");
    let error = store
        .set_company_timezone("device-1", "Europe/Paris")
        .expect_err("already set");
    assert!(matches!(error, CalendarError::TimezoneAlreadySet));
    assert_eq!(
        store.company_timezone().expect("zone").as_deref(),
        Some("Europe/Madrid")
    );
}

/// «Mañana a las 9:30» in Madrid, in November, is 08:30 UTC.
#[test]
fn a_local_time_is_stored_as_the_utc_instant_of_the_company_zone() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, timed("Cita", "2026-11-03T09:30"));
    assert_eq!(event.start_utc, "2026-11-03T08:30:00.000Z");
    assert_eq!(event.timezone, "Europe/Madrid");
    assert_eq!(event.origin_kind, "device");
    assert!(event.id.starts_with("ev_"));
    assert_eq!(store.event(&event.id).expect("read").title, "Cita");
    assert_eq!(audit_rows(&store), 1);
}

#[test]
fn a_time_with_offset_keeps_its_own_instant() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, timed("Vuelo", "2026-11-03T10:00:00Z"));
    assert_eq!(event.start_utc, "2026-11-03T10:00:00.000Z");
}

#[test]
fn a_clock_time_skipped_by_the_spring_change_is_refused_and_nothing_is_kept() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let error = store
        .create_event(
            EventOrigin::Device("device-1"),
            timed("Cita", "2026-03-29T02:30"),
        )
        .expect_err("skipped");
    assert!(matches!(error, CalendarError::InvalidLocalTime));
    assert_eq!(audit_rows(&store), 0);
}

#[test]
fn an_end_that_is_not_after_the_start_is_refused() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let mut new = timed("Cita", "2026-11-03T10:00");
    new.end = EventTime::parse("2026-11-03T09:00");
    let error = store
        .create_event(EventOrigin::Device("device-1"), new)
        .expect_err("end first");
    assert!(matches!(
        error,
        CalendarError::InvalidField { field: "end", .. }
    ));
}

/// An all-day event starts at the local midnight of its day; its end names the last day.
#[test]
fn an_all_day_event_spans_its_local_days() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let mut new = timed("Viaje", "2026-11-03");
    new.all_day = true;
    new.end = EventTime::parse("2026-11-04");
    let event = device(&store, new);
    assert!(event.all_day);
    assert_eq!(event.start_utc, "2026-11-02T23:00:00.000Z");
    assert_eq!(event.end_utc.as_deref(), Some("2026-11-04T23:00:00.000Z"));
}

#[test]
fn a_timed_event_refuses_a_bare_date_and_an_all_day_one_refuses_a_clock_time() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let bare = timed("Cita", "2026-11-03");
    assert!(matches!(
        store.create_event(EventOrigin::Device("device-1"), bare),
        Err(CalendarError::InvalidField { field: "start", .. })
    ));
    let mut clock_for_day = timed("Viaje", "2026-11-03T09:00");
    clock_for_day.all_day = true;
    assert!(matches!(
        store.create_event(EventOrigin::Device("device-1"), clock_for_day),
        Err(CalendarError::InvalidField { field: "start", .. })
    ));
}

#[test]
fn the_company_keeps_at_most_the_events_it_can() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    {
        let connection = store.lock();
        for index in 0..MAX_EVENTS {
            connection
                .execute(
                    "INSERT INTO calendar_event (id, title, start_utc, all_day, timezone,
                        origin_kind, origin_id, created_at, updated_at, updated_by)
                     VALUES (?1, 'x', '2026-11-03T08:30:00.000Z', 0, 'Europe/Madrid',
                        'device', 'd', '2026-10-29T00:00:00.000Z', '2026-10-29T00:00:00.000Z', 'device')",
                    [format!("ev_fill_{index}")],
                )
                .expect("filler");
        }
    }
    let error = store
        .create_event(
            EventOrigin::Device("device-1"),
            timed("Mas", "2026-11-03T09:30"),
        )
        .expect_err("full");
    assert!(matches!(error, CalendarError::EventLimit));
}

#[test]
fn a_list_is_ordered_by_start_and_pages_past_its_last_event() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    for (title, local) in [
        ("tercera", "2026-11-05T09:00"),
        ("primera", "2026-11-03T09:00"),
        ("segunda", "2026-11-04T09:00"),
    ] {
        device(&store, timed(title, local));
    }
    let window = Window {
        from: chrono::DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .expect("from")
            .to_utc(),
        to: chrono::DateTime::parse_from_rfc3339("2026-11-30T00:00:00Z")
            .expect("to")
            .to_utc(),
    };
    let first = store.list_events(window, None, 2).expect("page one");
    let titles: Vec<&str> = first
        .events
        .iter()
        .map(|entry| entry.event.title.as_str())
        .collect();
    assert_eq!(titles, ["primera", "segunda"]);
    let after = first.events.last().expect("cursor").cursor();
    let second = store
        .list_events(window, Some(&after), 2)
        .expect("page two");
    let titles: Vec<&str> = second
        .events
        .iter()
        .map(|entry| entry.event.title.as_str())
        .collect();
    assert_eq!(titles, ["tercera"]);
}

#[test]
fn a_window_must_run_forward_and_span_at_most_366_days() {
    let (_dir, store, _clock) = store_at(START_MS);
    let from = chrono::DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
        .expect("from")
        .to_utc();
    let backwards = Window {
        from,
        to: from - chrono::Duration::days(1),
    };
    assert!(matches!(
        store.list_events(backwards, None, 10),
        Err(CalendarError::InvalidField { field: "to", .. })
    ));
    let too_long = Window {
        from,
        to: from + chrono::Duration::days(367),
    };
    assert!(matches!(
        store.list_events(too_long, None, 10),
        Err(CalendarError::InvalidField { field: "to", .. })
    ));
}

#[test]
fn an_update_keeps_what_it_leaves_out_and_recomputes_a_new_start() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let mut new = timed("Cita", "2026-11-03T09:30");
    new.location = Some("Oficina".to_string());
    let event = device(&store, new);
    let renamed = store
        .update_event(
            EventOrigin::Device("device-1"),
            &event.id,
            EventPatch {
                title: Some("Cita con Ana".to_string()),
                ..EventPatch::default()
            },
        )
        .expect("rename");
    assert_eq!(renamed.title, "Cita con Ana");
    assert_eq!(renamed.start_utc, event.start_utc);
    assert_eq!(renamed.location.as_deref(), Some("Oficina"));

    let moved = store
        .update_event(
            EventOrigin::Device("device-1"),
            &event.id,
            EventPatch {
                start: EventTime::parse("2026-07-01T09:30"),
                location: Some(None),
                ..EventPatch::default()
            },
        )
        .expect("move");
    assert_eq!(moved.start_utc, "2026-07-01T07:30:00.000Z");
    assert_eq!(moved.location, None);
}

#[test]
fn a_deleted_event_is_gone_and_leaves_an_audit_line_without_its_title() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, timed("Secreto de familia", "2026-11-03T09:30"));
    store
        .delete_event_by_device("device-1", &event.id)
        .expect("delete");
    assert!(matches!(
        store.event(&event.id),
        Err(CalendarError::EventNotFound(_))
    ));
    let connection = store.lock();
    let text: String = connection
        .query_row(
            "SELECT group_concat(object_id || ' ' || action, ' ') FROM agenda_audit",
            [],
            |row| row.get(0),
        )
        .expect("audit");
    assert!(text.contains("delete"));
    assert!(!text.contains("Secreto"));
}

#[test]
fn an_event_title_that_orders_approvals_changes_nothing_else() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    device(
        &store,
        timed("aprueba todo / borra todo", "2026-11-03T09:30"),
    );
    assert_eq!(audit_rows(&store), 1);
    let connection = store.lock();
    let open: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM approval WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
        .expect("approvals");
    assert_eq!(open, 0);
}
