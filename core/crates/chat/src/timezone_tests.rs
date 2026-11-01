use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;

use super::{local_day_bounds, local_to_utc, parse_zone, utc_to_local, EventTime};
use crate::error::CalendarError;

fn local(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("a local time")
}

fn instant(text: &str) -> chrono::DateTime<Utc> {
    Utc.from_utc_datetime(&local(text))
}

#[test]
fn a_winter_time_in_madrid_is_one_hour_behind_utc() {
    let utc = local_to_utc(local("2026-11-03T09:30:00"), Madrid).expect("exists");
    assert_eq!(utc, instant("2026-11-03T08:30:00"));
}

#[test]
fn a_summer_time_in_madrid_is_two_hours_behind_utc() {
    let utc = local_to_utc(local("2026-07-01T09:30:00"), Madrid).expect("exists");
    assert_eq!(utc, instant("2026-07-01T07:30:00"));
}

/// 2026-03-29 02:00 jumps to 03:00 in Madrid: 02:30 never happens.
#[test]
fn a_local_time_skipped_by_the_spring_change_is_refused() {
    let error = local_to_utc(local("2026-03-29T02:30:00"), Madrid).expect_err("skipped");
    assert!(matches!(error, CalendarError::InvalidLocalTime));
}

/// 2026-10-25 03:00 falls back to 02:00, so 02:30 happens twice: the first one wins.
#[test]
fn a_local_time_repeated_by_the_autumn_change_takes_the_first() {
    let utc = local_to_utc(local("2026-10-25T02:30:00"), Madrid).expect("first");
    assert_eq!(utc, instant("2026-10-25T00:30:00"));
}

#[test]
fn utc_turns_back_into_the_local_clock_of_the_zone() {
    assert_eq!(
        utc_to_local(instant("2026-07-01T07:30:00"), Madrid),
        local("2026-07-01T09:30:00")
    );
}

/// The spring day has 23 hours: its bounds are local midnights, not 24 hours apart.
#[test]
fn a_spring_change_day_is_23_hours_long() {
    let day = NaiveDate::from_ymd_opt(2026, 3, 29).expect("day");
    let (start, end) = local_day_bounds(day, Madrid).expect("bounds");
    assert_eq!(start, instant("2026-03-28T23:00:00"));
    assert_eq!(end, instant("2026-03-29T22:00:00"));
    assert_eq!((end - start).num_hours(), 23);
}

#[test]
fn zone_names_must_be_iana_names() {
    assert!(parse_zone("Europe/Madrid").is_some());
    assert!(parse_zone("UTC").is_some());
    assert!(parse_zone("Mars/Olympus").is_none());
    assert!(parse_zone("").is_none());
    assert!(parse_zone("CET+1").is_none());
}

#[test]
fn an_event_time_without_offset_is_local() {
    let parsed = EventTime::parse("2026-11-03T09:30").expect("local");
    assert_eq!(parsed, EventTime::Local(local("2026-11-03T09:30:00")));
    let with_seconds = EventTime::parse("2026-11-03T09:30:15").expect("local");
    assert_eq!(with_seconds, EventTime::Local(local("2026-11-03T09:30:15")));
}

#[test]
fn an_event_time_with_offset_keeps_its_instant() {
    let parsed = EventTime::parse("2026-11-03T09:30+01:00").expect("offset");
    assert_eq!(parsed.fixed_instant(), Some(instant("2026-11-03T08:30:00")));
    let zulu = EventTime::parse("2026-11-03T08:30:00Z").expect("zulu");
    assert_eq!(zulu.fixed_instant(), Some(instant("2026-11-03T08:30:00")));
}

#[test]
fn a_date_alone_is_an_all_day_start() {
    let parsed = EventTime::parse("2026-11-03").expect("date");
    assert_eq!(
        parsed,
        EventTime::Date(NaiveDate::from_ymd_opt(2026, 11, 3).expect("day"))
    );
}

#[test]
fn malformed_event_times_are_refused() {
    for text in [
        "",
        "mañana 9:30",
        "2026-13-01T09:00",
        "2026-11-03 09:30",
        "09:30",
    ] {
        assert!(EventTime::parse(text).is_none(), "{text}");
    }
}
