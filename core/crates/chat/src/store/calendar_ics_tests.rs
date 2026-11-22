use chrono::{DateTime, Duration, Utc};
use rusqlite::params;

use super::calendar::EventOrigin;
use super::calendar_tests::{device, madrid, store_at, timed, START_MS};
use super::ChatStore;
use crate::error::CalendarError;
use crate::ics::{
    export_calendar, read_calendar,
    test_files::{calendar_of, vevent},
    ImportReport, SkipReason, MAX_EXPORT_EVENTS,
};
use crate::model_calendar::{CalendarEvent, NewEvent, MAX_EVENTS};
use crate::recurrence::Rule;
use crate::timezone::EventTime;

fn sample(index: usize) -> NewEvent {
    let day = 3 + index;
    match index % 5 {
        0 => NewEvent {
            title: format!("Feria {index}"),
            start: EventTime::parse(&format!("2026-11-{day:02}")).expect("date"),
            end: index
                .is_multiple_of(10)
                .then(|| EventTime::parse(&format!("2026-11-{:02}", day + 1)).expect("date")),
            all_day: true,
            location: None,
            notes: None,
            reminder_minutes: None,
            recurrence: None,
        },
        1 => NewEvent {
            location: Some("Sala 3".to_string()),
            notes: Some("uno, dos; tres\nlínea dos".to_string()),
            reminder_minutes: Some(15),
            ..timed("Revisión", &format!("2026-11-{day:02}T09:30"))
        },
        2 => NewEvent {
            recurrence: Some(Rule::parse("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=4").expect("rule")),
            ..timed("Serie", &format!("2026-11-{day:02}T09:00"))
        },
        3 => timed(
            "Reunión con Ñandú 🎉, ok; sí",
            &format!("2026-11-{day:02}T12:00"),
        ),
        _ => NewEvent {
            end: EventTime::parse(&format!("2026-11-{day:02}T11:30")),
            reminder_minutes: Some(0),
            ..timed("Con fin", &format!("2026-11-{day:02}T10:00"))
        },
    }
}

fn sample_company(store: &ChatStore) {
    madrid(store);
    for index in 0..20 {
        device(store, sample(index));
    }
}

fn import_text(store: &ChatStore, text: &[u8]) -> ImportReport {
    let parsed = read_calendar(text).expect("a calendar");
    store.import_events("device-2", parsed).expect("import")
}

fn exported_text(store: &ChatStore) -> String {
    let events = store.export_events(None, None).expect("export");
    export_calendar(&events).expect("text")
}

fn instant_at(minutes: i64) -> String {
    let start = DateTime::<Utc>::from_timestamp_millis(START_MS).expect("start");
    (start + Duration::minutes(minutes))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Rows written straight into the table, for the limits that a create would take long to reach.
fn insert_rows(store: &ChatStore, count: usize) {
    let mut connection = store.lock();
    let transaction = connection.transaction().expect("transaction");
    for index in 0..count {
        transaction
            .execute(
                "INSERT INTO calendar_event (id, title, start_utc, all_day, timezone,
                     origin_kind, origin_id, created_at, updated_at, updated_by)
                 VALUES (?1, 'Fila', ?2, 0, 'Europe/Madrid', 'device', 'device-1', ?3, ?3, 'device')",
                params![format!("ev_row_{index}"), instant_at(index as i64), instant_at(0)],
            )
            .expect("row");
    }
    transaction.commit().expect("commit");
}

#[test]
fn twenty_events_survive_an_export_and_an_import_into_an_empty_company() {
    let (_dir, source, _clock) = store_at(START_MS);
    sample_company(&source);
    let exported = source.export_events(None, None).expect("export");
    assert_eq!(exported.len(), 20);

    let (_other, target, _clock) = store_at(START_MS);
    madrid(&target);
    let report = import_text(
        &target,
        export_calendar(&exported).expect("text").as_bytes(),
    );
    assert_eq!(report.imported, 20);
    assert_eq!(report.skipped_existing, 0);
    assert!(report.details.is_empty());

    let copies = target.export_events(None, None).expect("export");
    for original in &exported {
        let uid = format!("{}@excubia", original.id);
        let copy = copies
            .iter()
            .find(|event| event.uid.as_deref() == Some(uid.as_str()))
            .expect("the same uid");
        assert_same_event(original, copy);
    }
}

fn assert_same_event(original: &CalendarEvent, copy: &CalendarEvent) {
    assert_eq!(copy.title, original.title);
    assert_eq!(copy.start_utc, original.start_utc);
    assert_eq!(copy.end_utc, original.end_utc);
    assert_eq!(copy.all_day, original.all_day);
    assert_eq!(copy.recurrence, original.recurrence);
    assert_eq!(copy.reminder_minutes, original.reminder_minutes);
    assert_eq!(copy.location, original.location);
    assert_eq!(copy.notes, original.notes);
    assert_eq!(copy.timezone, "Europe/Madrid");
    assert_eq!(copy.origin_kind, "import");
}

#[test]
fn importing_the_same_file_twice_does_not_duplicate() {
    let (_dir, source, _clock) = store_at(START_MS);
    sample_company(&source);
    let text = exported_text(&source);
    let (_other, target, _clock) = store_at(START_MS);
    madrid(&target);
    assert_eq!(import_text(&target, text.as_bytes()).imported, 20);
    let again = import_text(&target, text.as_bytes());
    assert_eq!(again.imported, 0);
    assert_eq!(again.skipped_existing, 20);
    assert_eq!(again.details.len(), 20);
    assert!(again
        .details
        .iter()
        .all(|skip| skip.reason == SkipReason::Exists));
    let total = target.export_events(None, None).expect("export").len();
    assert_eq!(total, 20);
}

#[test]
fn a_host_export_imported_into_its_own_company_is_all_existing() {
    let (_dir, store, _clock) = store_at(START_MS);
    sample_company(&store);
    let report = import_text(&store, exported_text(&store).as_bytes());
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped_existing, 20);
}

#[test]
fn an_imported_event_that_already_started_does_not_remind() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let file = calendar_of(&[vevent(&[
        "UID:pasada@x",
        "SUMMARY:Pasada",
        "DTSTART:20261001T090000Z",
        "BEGIN:VALARM",
        "TRIGGER:-PT15M",
        "ACTION:DISPLAY",
        "END:VALARM",
    ])]);
    assert_eq!(import_text(&store, &file).imported, 1);
    let events = store.export_events(None, None).expect("export");
    assert!(events[0].reminded_at.is_some());
    assert_eq!(store.deliver_due_reminders().expect("reminders"), 0);
}

#[test]
fn a_series_that_began_before_the_import_arms_only_its_next_occurrence() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let file = calendar_of(&[vevent(&[
        "UID:serie@x",
        "SUMMARY:Serie",
        "DTSTART:20261005T090000Z",
        "RRULE:FREQ=WEEKLY",
        "BEGIN:VALARM",
        "TRIGGER:-PT10M",
        "ACTION:DISPLAY",
        "END:VALARM",
    ])]);
    assert_eq!(import_text(&store, &file).imported, 1);
    let series = &store.export_events(None, None).expect("export")[0];
    assert!(series.reminded_occurrence.is_some());
    assert!(series.reminded_at.is_none());
}

#[test]
fn a_company_without_a_zone_imports_and_exports_nothing() {
    let (_dir, store, _clock) = store_at(START_MS);
    let file = calendar_of(&[vevent(&["SUMMARY:Hola", "DTSTART:20261103T083000Z"])]);
    let parsed = read_calendar(&file).expect("a calendar");
    assert!(matches!(
        store.import_events("device-2", parsed),
        Err(CalendarError::TimezoneUnset)
    ));
    assert!(matches!(
        store.export_events(None, None),
        Err(CalendarError::TimezoneUnset)
    ));
}

#[test]
fn events_the_calendar_cannot_keep_are_listed_by_index_and_reason() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let file = calendar_of(&[
        vevent(&["SUMMARY:Uno", "DTSTART:20261103T083000Z"]),
        vevent(&["SUMMARY:Hueco", "DTSTART:20260329T023000"]),
        vevent(&[
            "SUMMARY:Cambio",
            "DTSTART:20261103T083000Z",
            "RECURRENCE-ID:20261110T083000Z",
        ]),
        vevent(&[
            "SUMMARY:Serie",
            "DTSTART:20261103T083000Z",
            "RRULE:FREQ=DAILY",
            "EXDATE:20261104T083000Z",
        ]),
        vevent(&["SUMMARY:Cuatro", "DTSTART:20261104T083000Z"]),
    ]);
    let report = import_text(&store, &file);
    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped_existing, 0);
    assert_eq!(report.skipped_unsupported, 3);
    let reasons: Vec<(usize, SkipReason)> = report
        .details
        .iter()
        .map(|skip| (skip.index, skip.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![
            (1, SkipReason::InvalidTime),
            (2, SkipReason::ExceptionInstance),
            (3, SkipReason::UnsupportedRecurrence),
        ]
    );
}

#[test]
fn the_company_keeps_no_more_than_5000_events() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    insert_rows(&store, MAX_EVENTS as usize - 1);
    let file = calendar_of(&[
        vevent(&["SUMMARY:Uno", "DTSTART:20261103T083000Z"]),
        vevent(&["SUMMARY:Dos", "DTSTART:20261104T083000Z"]),
    ]);
    let report = import_text(&store, &file);
    assert_eq!(report.imported, 1);
    assert_eq!(report.details.len(), 1);
    assert_eq!(report.details[0].reason, SkipReason::CalendarLimit);
}

#[test]
fn an_export_holds_at_most_2000_events_and_a_window_narrows_it() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    insert_rows(&store, MAX_EXPORT_EVENTS + 1);
    assert!(matches!(
        store.export_events(None, None),
        Err(CalendarError::TooManyEvents)
    ));
    let from = EventTime::parse("2026-10-29T00:00:00Z");
    let to = EventTime::parse("2026-10-29T01:00:00Z");
    assert_eq!(store.export_events(from, to).expect("window").len(), 60);
}

#[test]
fn an_export_window_takes_the_events_that_start_in_it_and_the_series_with_an_occurrence_there() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let plain = device(&store, timed("Llamada", "2026-11-03T09:30"));
    let series = device(
        &store,
        NewEvent {
            recurrence: Some(Rule::parse("FREQ=WEEKLY;COUNT=10").expect("rule")),
            ..timed("Semanal", "2026-10-01T09:00")
        },
    );
    device(
        &store,
        NewEvent {
            recurrence: Some(Rule::parse("FREQ=WEEKLY;COUNT=2").expect("rule")),
            ..timed("Acabada", "2026-10-01T10:00")
        },
    );
    let ids = |from: &str, to: &str| -> Vec<String> {
        store
            .export_events(EventTime::parse(from), EventTime::parse(to))
            .expect("window")
            .into_iter()
            .map(|event| event.id)
            .collect()
    };
    assert_eq!(ids("2026-11-03", "2026-11-04"), vec![plain.id]);
    assert_eq!(ids("2026-11-05", "2026-11-06"), vec![series.id]);
}

#[test]
fn an_export_window_must_end_after_it_starts() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let from = EventTime::parse("2026-11-04");
    let to = EventTime::parse("2026-11-03");
    assert!(matches!(
        store.export_events(from, to),
        Err(CalendarError::InvalidField { field: "to", .. })
    ));
}

#[test]
fn an_event_created_by_the_host_keeps_its_origin_and_gets_its_uid_on_export() {
    let (_dir, store, _clock) = store_at(START_MS);
    madrid(&store);
    let created = store
        .create_event(
            EventOrigin::Device("device-1"),
            timed("Local", "2026-11-03T09:30"),
        )
        .expect("event");
    let event = store.event(&created.id).expect("event");
    assert_eq!(event.uid, None);
    assert_eq!(event.origin_kind, "device");
    assert!(exported_text(&store).contains(&format!("UID:{}@excubia", created.id)));
}
