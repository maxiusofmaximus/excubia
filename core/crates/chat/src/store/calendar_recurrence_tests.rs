//! The store's side of recurring events (CALENDARIO-F4b): a series is written, listed by its
//! occurrences, summarised, reminded once per occurrence, and refused when its UNTIL is void.

use super::calendar::EventOrigin;
use super::calendar_remind_tests::ms_of;
use super::calendar_tests::{conversation, device, madrid, store_at, timed, START_MS};
use super::NextEvent;
use crate::error::CalendarError;
use crate::model_calendar::{EventPatch, NewEvent, Window};
use crate::recurrence::Rule;

/// A timed event that repeats by `rule`, starting at `local` in the company's zone.
fn series(local: &str, rule: &str) -> NewEvent {
    let mut new = timed("Clase", local);
    new.recurrence = Some(Rule::parse(rule).expect("rule"));
    new
}

/// November 2026, the month every test lists.
fn november() -> Window {
    Window {
        from: chrono::DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .expect("from")
            .to_utc(),
        to: chrono::DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z")
            .expect("to")
            .to_utc(),
    }
}

/// A daily series lists each occurrence with its own start; every entry carries the series' id.
#[test]
fn a_series_lists_each_occurrence_with_its_own_start() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=3"));
    let listing = store.list_events(november(), None, 10).expect("list");
    assert!(!listing.truncated);
    let starts: Vec<&str> = listing
        .events
        .iter()
        .map(|entry| entry.occurrence_start.as_str())
        .collect();
    assert_eq!(
        starts,
        [
            "2026-11-02T08:00:00.000Z",
            "2026-11-03T08:00:00.000Z",
            "2026-11-04T08:00:00.000Z"
        ]
    );
    assert!(listing
        .events
        .iter()
        .all(|entry| entry.recurring && entry.event.id == event.id));
    assert_eq!(
        listing.events[1].event.start_utc,
        "2026-11-03T08:00:00.000Z"
    );
}

/// Plain events and occurrences form one list in start order.
#[test]
fn plain_events_and_occurrences_share_one_list() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=2"));
    device(&store, timed("Dentista", "2026-11-03T12:00"));
    let listing = store.list_events(november(), None, 10).expect("list");
    let kinds: Vec<(&str, bool)> = listing
        .events
        .iter()
        .map(|entry| (entry.event.title.as_str(), entry.recurring))
        .collect();
    assert_eq!(
        kinds,
        [("Clase", true), ("Clase", true), ("Dentista", false)]
    );
}

/// A page that stops inside a series resumes after that occurrence, by its `id@start` cursor.
#[test]
fn a_page_resumes_after_an_occurrence_with_its_cursor() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=3"));
    let first = store.list_events(november(), None, 2).expect("page one");
    assert!(first.truncated);
    let after = first.events.last().expect("last").cursor();
    assert_eq!(after.split('@').nth(1), Some("2026-11-03T08:00:00.000Z"));
    let second = store
        .list_events(november(), Some(&after), 2)
        .expect("page two");
    let starts: Vec<&str> = second
        .events
        .iter()
        .map(|entry| entry.occurrence_start.as_str())
        .collect();
    assert_eq!(starts, ["2026-11-04T08:00:00.000Z"]);
    assert!(!second.truncated);
}

/// A cursor that names no event, or a broken instant, is refused.
#[test]
fn a_cursor_that_names_nothing_is_refused() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=3"));
    assert!(matches!(
        store.list_events(november(), Some("ev_missing"), 10),
        Err(CalendarError::InvalidField { field: "after", .. })
    ));
    assert!(matches!(
        store.list_events(november(), Some("ev_x@no-es-hora"), 10),
        Err(CalendarError::InvalidField { field: "after", .. })
    ));
}

/// A series whose UNTIL falls before its first occurrence is refused, and nothing is kept.
#[test]
fn a_series_that_ends_before_it_starts_is_refused() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let error = store
        .create_event(
            EventOrigin::Device("device-1"),
            series("2026-11-02T09:00", "FREQ=DAILY;UNTIL=20261001"),
        )
        .expect_err("void until");
    assert!(matches!(
        error,
        CalendarError::InvalidField {
            field: "recurrence",
            ..
        }
    ));
    assert!(store
        .list_events(november(), None, 10)
        .expect("list")
        .events
        .is_empty());
}

/// An edit that clears the rule turns the series back into one event.
#[test]
fn an_edit_can_clear_the_rule_and_leave_one_event() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=3"));
    store
        .update_event(
            EventOrigin::Device("device-1"),
            &event.id,
            EventPatch {
                recurrence: Some(None),
                ..EventPatch::default()
            },
        )
        .expect("clear");
    let listing = store.list_events(november(), None, 10).expect("list");
    assert_eq!(listing.events.len(), 1);
    assert!(!listing.events[0].recurring);
}

/// The summary names the next occurrence, and today counts the occurrences of the local day.
#[test]
fn the_summary_names_the_next_occurrence_and_counts_the_day() {
    let (_dir, store, clock) = store_at(START_MS);
    madrid(&store);
    let event = device(&store, series("2026-11-02T09:00", "FREQ=DAILY;COUNT=3"));
    let summary = store.calendar_summary().expect("summary");
    assert_eq!(summary.today, 0);
    assert_eq!(
        summary.next,
        Some(NextEvent {
            id: event.id.clone(),
            start_utc: "2026-11-02T08:00:00.000Z".to_string(),
        })
    );
    clock.set(ms_of("2026-11-02T10:00:00.000Z"));
    let summary = store.calendar_summary().expect("summary");
    assert_eq!(summary.today, 1);
    assert_eq!(
        summary.next.expect("next").start_utc,
        "2026-11-03T08:00:00.000Z"
    );
}

/// A series reminds once per occurrence; each card names its own occurrence's time.
#[test]
fn a_series_reminds_once_per_occurrence_with_its_own_card() {
    let (_dir, store, clock) = store_at(START_MS);
    madrid(&store);
    let conversation_id = conversation(&store);
    let mut new = series("2026-11-02T09:00", "FREQ=DAILY;COUNT=2");
    new.reminder_minutes = Some(30);
    device(&store, new);
    clock.set(ms_of("2026-11-02T07:30:00.000Z"));
    assert_eq!(store.deliver_due_reminders().expect("first"), 1);
    assert_eq!(store.deliver_due_reminders().expect("again"), 0);
    clock.set(ms_of("2026-11-03T07:30:00.000Z"));
    assert_eq!(store.deliver_due_reminders().expect("second"), 1);
    let page = store
        .timeline(&conversation_id, None, 20)
        .expect("timeline");
    let summaries: Vec<String> = page
        .entries
        .iter()
        .filter(|entry| entry.kind == "card")
        .filter_map(|entry| entry.summary.clone())
        .collect();
    assert_eq!(summaries, ["a las 09:00", "a las 09:00"]);
}
