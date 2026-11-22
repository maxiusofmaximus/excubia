use chrono::{NaiveDate, NaiveDateTime};

use super::import::Draft;
use super::test_files::{calendar_of, vevent};
use super::{read_calendar, IcsError, SkipReason, MAX_VEVENTS};
use crate::recurrence::Rule;
use crate::timezone::EventTime;

const START: &str = "DTSTART:20261103T083000Z";

/// The outcome of the one event a file holds.
fn one(props: &[&str]) -> Result<Draft, SkipReason> {
    let file = calendar_of(&[vevent(props)]);
    read_calendar(&file).expect("a calendar").remove(0).outcome
}

fn instant(text: &str) -> EventTime {
    EventTime::Offset(chrono::DateTime::parse_from_rfc3339(text).expect("an instant"))
}

#[test]
fn a_uid_is_kept_and_a_missing_one_is_made_from_the_content() {
    let draft = one(&["UID:abc@x", "SUMMARY:Hola", START]).expect("an event");
    assert_eq!(draft.uid, "abc@x");
    let made = one(&["SUMMARY:Hola", START]).expect("an event");
    assert!(made.uid.starts_with("imp-"), "{}", made.uid);
    assert_eq!(made.uid.len(), 36);
    let again = one(&["SUMMARY:Hola", START]).expect("an event");
    assert_eq!(made.uid, again.uid, "the same content makes the same uid");
}

#[test]
fn a_summary_is_required() {
    assert_eq!(one(&[START]).err(), Some(SkipReason::InvalidText));
    assert_eq!(
        one(&["SUMMARY:   ", START]).err(),
        Some(SkipReason::InvalidText)
    );
}

#[test]
fn a_missing_start_is_no_start() {
    assert_eq!(one(&["SUMMARY:Hola"]).err(), Some(SkipReason::NoStart));
}

#[test]
fn an_exception_instance_is_omitted_with_its_reason() {
    let props = ["SUMMARY:Cambio", START, "RECURRENCE-ID:20261110T083000Z"];
    assert_eq!(one(&props).err(), Some(SkipReason::ExceptionInstance));
}

#[test]
fn excluded_and_extra_dates_are_unsupported_recurrence() {
    let exdate = [
        "SUMMARY:Serie",
        START,
        "RRULE:FREQ=DAILY",
        "EXDATE:20261104T083000Z",
    ];
    assert_eq!(one(&exdate).err(), Some(SkipReason::UnsupportedRecurrence));
    let rdate = ["SUMMARY:Serie", START, "RDATE:20261110T083000Z"];
    assert_eq!(one(&rdate).err(), Some(SkipReason::UnsupportedRecurrence));
}

#[test]
fn a_rule_outside_the_subset_is_unsupported_and_a_broken_one_is_invalid() {
    let outside = ["SUMMARY:Serie", START, "RRULE:FREQ=MONTHLY;BYMONTHDAY=3"];
    assert_eq!(one(&outside).err(), Some(SkipReason::UnsupportedRecurrence));
    let hourly = ["SUMMARY:Serie", START, "RRULE:FREQ=HOURLY"];
    assert_eq!(one(&hourly).err(), Some(SkipReason::UnsupportedRecurrence));
    let broken = ["SUMMARY:Serie", START, "RRULE:FREQ=FOO"];
    assert_eq!(one(&broken).err(), Some(SkipReason::InvalidRecurrence));
}

#[test]
fn a_supported_rule_is_kept_normalized() {
    let draft = one(&[
        "SUMMARY:Serie",
        START,
        "RRULE:FREQ=WEEKLY;BYDAY=WE,MO;COUNT=5",
    ])
    .expect("an event");
    let rule = draft.event.recurrence.expect("the rule");
    assert_eq!(
        rule,
        Rule::parse("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5").expect("rule")
    );
}

#[test]
fn the_first_valarm_gives_the_reminder_in_minutes() {
    let alarm = "BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM";
    let file =
        calendar_of(&[vevent(&["SUMMARY:Hola", START])
            .replace("END:VEVENT", &format!("{alarm}\r\nEND:VEVENT"))]);
    let draft = read_calendar(&file).expect("a calendar").remove(0).outcome;
    assert_eq!(draft.expect("an event").event.reminder_minutes, Some(15));
}

#[test]
fn a_reminder_that_is_not_a_kept_minute_count_gives_no_reminder() {
    for trigger in [
        "TRIGGER:PT15M",
        "TRIGGER;RELATED=END:-PT15M",
        "TRIGGER:-PT90S",
    ] {
        let alarm = format!("BEGIN:VALARM\r\n{trigger}\r\nACTION:DISPLAY\r\nEND:VALARM");
        let file = calendar_of(&[vevent(&["SUMMARY:Hola", START])
            .replace("END:VEVENT", &format!("{alarm}\r\nEND:VEVENT"))]);
        let draft = read_calendar(&file).expect("a calendar").remove(0).outcome;
        assert_eq!(
            draft.expect("an event").event.reminder_minutes,
            None,
            "{trigger}"
        );
    }
}

#[test]
fn a_timed_event_with_a_duration_ends_after_it_starts() {
    let draft = one(&["SUMMARY:Hola", START, "DURATION:PT1H"]).expect("an event");
    assert_eq!(draft.event.end, Some(instant("2026-11-03T09:30:00Z")));
    assert!(!draft.event.all_day);
}

#[test]
fn an_all_day_end_is_exclusive_in_the_file_and_the_last_day_here() {
    let props = [
        "SUMMARY:Feria",
        "DTSTART;VALUE=DATE:20261103",
        "DTEND;VALUE=DATE:20261105",
    ];
    let draft = one(&props).expect("an event");
    assert!(draft.event.all_day);
    let day = NaiveDate::from_ymd_opt(2026, 11, 4).expect("day");
    assert_eq!(draft.event.end, Some(EventTime::Date(day)));
}

#[test]
fn an_all_day_event_of_one_day_has_no_end() {
    let no_end = one(&["SUMMARY:Feria", "DTSTART;VALUE=DATE:20261103"]).expect("an event");
    assert_eq!(no_end.event.end, None);
    let same_day = one(&[
        "SUMMARY:Feria",
        "DTSTART;VALUE=DATE:20261103",
        "DTEND;VALUE=DATE:20261103",
    ])
    .expect("an event");
    assert_eq!(same_day.event.end, None);
}

#[test]
fn an_all_day_duration_counts_whole_days() {
    let props = [
        "SUMMARY:Feria",
        "DTSTART;VALUE=DATE:20261103",
        "DURATION:P2D",
    ];
    let draft = one(&props).expect("an event");
    let day = NaiveDate::from_ymd_opt(2026, 11, 4).expect("day");
    assert_eq!(draft.event.end, Some(EventTime::Date(day)));
}

#[test]
fn a_date_start_with_a_clock_end_is_invalid_time() {
    let props = [
        "SUMMARY:Feria",
        "DTSTART;VALUE=DATE:20261103",
        "DTEND:20261104T100000Z",
    ];
    assert_eq!(one(&props).err(), Some(SkipReason::InvalidTime));
}

#[test]
fn an_end_is_read_as_given_and_the_store_checks_its_order() {
    let props = ["SUMMARY:Hola", START, "DTEND:20261103T083000Z"];
    let draft = one(&props).expect("an event");
    assert_eq!(draft.event.end, Some(instant("2026-11-03T08:30:00Z")));
}

#[test]
fn a_floating_start_stays_local_and_a_zoned_start_becomes_its_instant() {
    let floating = one(&["SUMMARY:Hola", "DTSTART:20261103T093000"]).expect("an event");
    let local =
        NaiveDateTime::parse_from_str("2026-11-03T09:30:00", "%Y-%m-%dT%H:%M:%S").expect("clock");
    assert_eq!(floating.event.start, EventTime::Local(local));
    let zoned =
        one(&["SUMMARY:Hola", "DTSTART;TZID=Europe/Madrid:20261103T093000"]).expect("an event");
    assert_eq!(zoned.event.start, instant("2026-11-03T08:30:00Z"));
}

#[test]
fn an_invalid_start_is_invalid_time() {
    assert_eq!(
        one(&["SUMMARY:Hola", "DTSTART:20261399T000000Z"]).err(),
        Some(SkipReason::InvalidTime)
    );
}

#[test]
fn text_is_cut_to_the_calendar_limits_and_control_characters_become_spaces() {
    let long = "á".repeat(150);
    let summary = format!("SUMMARY:{long}\\nfin");
    let draft = one(&[summary.as_str(), START]).expect("an event");
    assert_eq!(draft.event.title.chars().count(), 120);
    let place = format!("LOCATION:{}", "x".repeat(250));
    let draft = one(&["SUMMARY:Hola", START, place.as_str()]).expect("an event");
    assert_eq!(draft.event.location.expect("a place").chars().count(), 200);
    let notes = format!("DESCRIPTION:{}", "ñ".repeat(3_000));
    let draft = one(&["SUMMARY:Hola", START, notes.as_str()]).expect("an event");
    assert_eq!(draft.event.notes.expect("notes").len(), 4_096);
}

#[test]
fn escapes_are_read_when_the_draft_is_made() {
    let draft = one(&[
        "SUMMARY:Cena\\, con Ana\\; ñ",
        START,
        "DESCRIPTION:Primera\\nSegunda",
    ])
    .expect("an event");
    assert_eq!(draft.event.title, "Cena, con Ana; ñ");
    assert_eq!(draft.event.notes.as_deref(), Some("Primera\nSegunda"));
}

#[test]
fn unknown_and_vendor_properties_and_other_components_are_ignored() {
    let file = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Madrid\r\nEND:VTIMEZONE\r\n\
        BEGIN:VTODO\r\nSUMMARY:Tarea\r\nEND:VTODO\r\n\
        BEGIN:VEVENT\r\nUID:a@x\r\nSUMMARY:Hola\r\nX-GOOGLE-CONFERENCE:x\r\n\
        ATTENDEE;CN=Ana:mailto:ana@example.com\r\nORGANIZER:mailto:o@example.com\r\n\
        CATEGORIES:trabajo\r\nURL:https://example.com\r\nDTSTART:20261103T083000Z\r\n\
        END:VEVENT\r\nEND:VCALENDAR\r\n";
    let parsed = read_calendar(file.as_bytes()).expect("a calendar");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].outcome.as_ref().expect("an event").uid, "a@x");
}

#[test]
fn a_bad_event_does_not_stop_the_rest_and_keeps_its_index() {
    let file = calendar_of(&[
        vevent(&["SUMMARY:Uno", START]),
        vevent(&["SUMMARY:Sin inicio"]),
        vevent(&["SUMMARY:Tres", START]),
    ]);
    let parsed = read_calendar(&file).expect("a calendar");
    let indexes: Vec<usize> = parsed.iter().map(|entry| entry.index).collect();
    assert_eq!(indexes, vec![0, 1, 2]);
    assert!(parsed[0].outcome.is_ok());
    assert_eq!(parsed[1].outcome.as_ref().err(), Some(&SkipReason::NoStart));
    assert!(parsed[2].outcome.is_ok());
}

#[test]
fn five_hundred_events_are_read_and_five_hundred_and_one_are_refused() {
    let event = vevent(&["SUMMARY:Hola", START]);
    let at_limit = calendar_of(&vec![event.clone(); MAX_VEVENTS]);
    assert_eq!(read_calendar(&at_limit).expect("a calendar").len(), 500);
    let over = calendar_of(&vec![event; MAX_VEVENTS + 1]);
    assert_eq!(read_calendar(&over), Err(IcsError::TooManyEvents));
}

/// A file as a mail client writes one: a zone block, vendor properties, folded lines, an
/// exception of a series and a rule the calendar does not keep.
#[test]
fn a_file_from_a_mail_client_is_read_event_by_event() {
    let parsed = read_calendar(include_bytes!("testdata/google-outlook.ics")).expect("a calendar");
    let outcomes: Vec<(usize, Option<SkipReason>)> = parsed
        .iter()
        .map(|entry| (entry.index, entry.outcome.as_ref().err().copied()))
        .collect();
    assert_eq!(
        outcomes,
        vec![
            (0, None),
            (1, None),
            (2, None),
            (3, Some(SkipReason::ExceptionInstance)),
            (4, Some(SkipReason::UnsupportedRecurrence)),
        ]
    );
    let timed = parsed[0].outcome.as_ref().expect("the zoned event");
    assert_eq!(timed.uid, "google-1@google.com");
    assert_eq!(timed.event.reminder_minutes, Some(15));
    assert!(!timed.event.all_day);
    assert!(parsed[1].outcome.as_ref().expect("the day").event.all_day);
}
