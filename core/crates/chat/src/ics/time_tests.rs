use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};

use super::reader::{calendar, Property};
use super::time::{duration, stamp, trigger_minutes, Stamp};
use crate::ics::SkipReason;

/// The one property a line of a calendar names, read the way the file reader reads it.
fn property(line: &str) -> Property {
    let text = format!("BEGIN:VCALENDAR\r\n{line}\r\nEND:VCALENDAR\r\n");
    calendar(text.as_bytes())
        .expect("a calendar")
        .props
        .into_iter()
        .next()
        .expect("the property")
}

fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("an instant")
        .with_timezone(&Utc)
}

fn naive(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("a clock reading")
}

#[test]
fn a_date_is_an_all_day_stamp() {
    assert_eq!(
        stamp(&property("DTSTART;VALUE=DATE:20261103")),
        Ok(Stamp::Date(
            NaiveDate::from_ymd_opt(2026, 11, 3).expect("day")
        ))
    );
}

#[test]
fn a_z_stamp_is_the_instant_it_names() {
    assert_eq!(
        stamp(&property("DTSTART:20261103T083000Z")),
        Ok(Stamp::Utc(utc("2026-11-03T08:30:00Z")))
    );
}

#[test]
fn a_tzid_clock_is_converted_with_the_offset_of_its_day() {
    assert_eq!(
        stamp(&property("DTSTART;TZID=Europe/Madrid:20260701T090000")),
        Ok(Stamp::Utc(utc("2026-07-01T07:00:00Z")))
    );
    assert_eq!(
        stamp(&property("DTSTART;TZID=Europe/Madrid:20261103T093000")),
        Ok(Stamp::Utc(utc("2026-11-03T08:30:00Z")))
    );
}

#[test]
fn a_clock_without_zone_is_floating() {
    assert_eq!(
        stamp(&property("DTSTART:20261103T093000")),
        Ok(Stamp::Floating(naive("2026-11-03T09:30:00")))
    );
}

#[test]
fn an_unknown_zone_or_a_clock_skipped_by_the_change_is_invalid_time() {
    assert_eq!(
        stamp(&property("DTSTART;TZID=Mars/Olympus:20261103T093000")),
        Err(SkipReason::InvalidTime)
    );
    assert_eq!(
        stamp(&property("DTSTART;TZID=Europe/Madrid:20260329T023000")),
        Err(SkipReason::InvalidTime)
    );
}

#[test]
fn a_malformed_stamp_is_invalid_time() {
    assert_eq!(
        stamp(&property("DTSTART:2026-11-03")),
        Err(SkipReason::InvalidTime)
    );
    assert_eq!(
        stamp(&property("DTSTART:20261399T000000Z")),
        Err(SkipReason::InvalidTime)
    );
}

#[test]
fn a_duration_reads_weeks_days_hours_minutes_and_seconds() {
    assert_eq!(duration("P1D"), Some(Duration::days(1)));
    assert_eq!(duration("P1W"), Some(Duration::weeks(1)));
    assert_eq!(duration("PT1H30M"), Some(Duration::minutes(90)));
    assert_eq!(duration("P1DT2H"), Some(Duration::hours(26)));
    assert_eq!(duration("PT45S"), Some(Duration::seconds(45)));
    assert_eq!(duration("+P1D"), Some(Duration::days(1)));
}

#[test]
fn a_negative_duration_is_negative() {
    assert_eq!(duration("-PT15M"), Some(Duration::minutes(-15)));
}

#[test]
fn a_malformed_duration_is_none() {
    for text in ["P", "PT", "P1X", "PT1.5H", "1D", "PT1H2D", ""] {
        assert_eq!(duration(text), None, "{text:?}");
    }
}

#[test]
fn a_reminder_trigger_is_minutes_before_the_start() {
    assert_eq!(trigger_minutes("-PT15M"), Some(15));
    assert_eq!(trigger_minutes("-PT1H"), Some(60));
    assert_eq!(trigger_minutes("-P1D"), Some(1440));
    assert_eq!(trigger_minutes("-PT0M"), Some(0));
    assert_eq!(trigger_minutes("-PT10080M"), Some(10_080));
}

#[test]
fn a_trigger_the_calendar_does_not_keep_gives_no_reminder() {
    assert_eq!(trigger_minutes("PT15M"), None, "after the start");
    assert_eq!(trigger_minutes("-PT10081M"), None, "past one week");
    assert_eq!(trigger_minutes("-PT90S"), None, "not whole minutes");
    assert_eq!(
        trigger_minutes("20261103T090000Z"),
        None,
        "an absolute time"
    );
}
