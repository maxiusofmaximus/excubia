use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use chrono_tz::Europe::Madrid;
use chrono_tz::Tz;

use super::{occurrences, Series, MAX_STEPS};
use crate::recurrence::Rule;
use crate::timezone::utc_to_local;

fn local(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("local time")
}

fn instant(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("instant")
        .with_timezone(&Utc)
}

/// A series that starts at `first` (Madrid clock) and lasts `span` minutes.
fn series(rule: &str, first: &str, span: Option<i64>, zone: Tz) -> Series {
    Series {
        rule: Rule::parse(rule).expect("rule"),
        first: local(first),
        span: span.map(Duration::minutes),
        zone,
    }
}

/// The Madrid clock of every occurrence found in `[from, to)`, as `YYYY-MM-DD HH:MM`.
fn clocks(series: &Series, from: &str, to: &str, max: usize) -> Vec<String> {
    let mut steps = MAX_STEPS;
    occurrences(series, instant(from), instant(to), max, &mut steps)
        .occurrences
        .iter()
        .map(|found| {
            utc_to_local(found.start, series.zone)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .collect()
}

const FAR: &str = "2100-01-01T00:00:00Z";
const EPOCH: &str = "2000-01-01T00:00:00Z";

#[test]
fn a_daily_rule_repeats_on_the_local_clock_of_its_start() {
    let daily = series("FREQ=DAILY;COUNT=3", "2026-11-01 09:30", Some(60), Madrid);
    assert_eq!(
        clocks(&daily, EPOCH, FAR, 10),
        ["2026-11-01 09:30", "2026-11-02 09:30", "2026-11-03 09:30"]
    );
    let mut steps = MAX_STEPS;
    let found = occurrences(&daily, instant(EPOCH), instant(FAR), 1, &mut steps);
    assert_eq!(found.occurrences[0].start, instant("2026-11-01T08:30:00Z"));
    assert_eq!(
        found.occurrences[0].end,
        Some(instant("2026-11-01T09:30:00Z"))
    );
}

#[test]
fn a_weekly_rule_with_interval_and_days_skips_the_weeks_it_does_not_name() {
    let weekly = series(
        "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=5",
        "2026-11-02 09:30",
        None,
        Madrid,
    );
    assert_eq!(
        clocks(&weekly, EPOCH, FAR, 10),
        [
            "2026-11-02 09:30",
            "2026-11-04 09:30",
            "2026-11-16 09:30",
            "2026-11-18 09:30",
            "2026-11-30 09:30",
        ]
    );
}

/// A start that is a Wednesday does not get the Monday of its own week.
#[test]
fn the_first_week_holds_no_day_before_the_start() {
    let weekly = series("FREQ=WEEKLY;BYDAY=MO,WE", "2026-11-04 09:30", None, Madrid);
    assert_eq!(
        clocks(&weekly, EPOCH, FAR, 2),
        ["2026-11-04 09:30", "2026-11-09 09:30"]
    );
}

/// A monthly rule on the 31st skips the months that have no 31st, and COUNT counts only
/// the occurrences that exist.
#[test]
fn a_monthly_rule_on_the_31st_skips_the_short_months() {
    let monthly = series("FREQ=MONTHLY;COUNT=6", "2026-01-31 09:00", None, Madrid);
    assert_eq!(
        clocks(&monthly, EPOCH, FAR, 10),
        [
            "2026-01-31 09:00",
            "2026-03-31 09:00",
            "2026-05-31 09:00",
            "2026-07-31 09:00",
            "2026-08-31 09:00",
            "2026-10-31 09:00",
        ]
    );
}

/// A yearly rule on 29 February keeps only the leap years.
#[test]
fn a_yearly_rule_on_the_29th_of_february_keeps_the_leap_years() {
    let yearly = series("FREQ=YEARLY;COUNT=3", "2024-02-29 10:00", None, Madrid);
    assert_eq!(
        clocks(&yearly, EPOCH, FAR, 10),
        ["2024-02-29 10:00", "2028-02-29 10:00", "2032-02-29 10:00"]
    );
}

#[test]
fn until_as_a_date_includes_that_local_day() {
    let daily = series(
        "FREQ=DAILY;UNTIL=20261103",
        "2026-11-01 09:30",
        None,
        Madrid,
    );
    assert_eq!(
        clocks(&daily, EPOCH, FAR, 10),
        ["2026-11-01 09:30", "2026-11-02 09:30", "2026-11-03 09:30"]
    );
}

#[test]
fn until_as_an_instant_stops_at_that_instant() {
    let daily = series(
        "FREQ=DAILY;UNTIL=20261102T083000Z",
        "2026-11-01 09:30",
        None,
        Madrid,
    );
    assert_eq!(
        clocks(&daily, EPOCH, FAR, 10),
        ["2026-11-01 09:30", "2026-11-02 09:30"]
    );
}

#[test]
fn a_window_cuts_the_series_at_both_ends() {
    let daily = series("FREQ=DAILY", "2026-11-01 09:30", None, Madrid);
    assert_eq!(
        clocks(&daily, "2026-11-05T00:00:00Z", "2026-11-07T00:00:00Z", 10),
        ["2026-11-05 09:30", "2026-11-06 09:30"]
    );
}

#[test]
fn max_keeps_the_earliest_occurrences_only() {
    let daily = series("FREQ=DAILY", "2026-11-01 09:30", None, Madrid);
    assert_eq!(
        clocks(&daily, EPOCH, FAR, 2),
        ["2026-11-01 09:30", "2026-11-02 09:30"]
    );
}

/// A window far from the start is reached without walking every day since then.
#[test]
fn a_window_far_from_the_start_is_reached_by_jumping() {
    let daily = series("FREQ=DAILY", "1998-01-01 09:30", None, Madrid);
    let mut steps = MAX_STEPS;
    let found = occurrences(
        &daily,
        instant("2026-10-29T00:00:00Z"),
        instant("2026-10-31T00:00:00Z"),
        10,
        &mut steps,
    );
    assert!(!found.exhausted);
    assert_eq!(found.occurrences.len(), 2);
}

/// A counted series walks from its start, so a small budget runs out before the window.
#[test]
fn a_budget_that_runs_out_is_reported_and_keeps_what_was_found() {
    let counted = series("FREQ=DAILY;COUNT=500", "2026-01-01 09:30", None, Madrid);
    let mut steps = 10;
    let found = occurrences(
        &counted,
        instant("2026-12-01T00:00:00Z"),
        instant("2027-01-01T00:00:00Z"),
        100,
        &mut steps,
    );
    assert!(found.exhausted);
    assert!(found.occurrences.is_empty());
}

#[test]
fn an_empty_or_reversed_window_finds_nothing() {
    let daily = series("FREQ=DAILY", "2026-11-01 09:30", None, Madrid);
    assert!(clocks(&daily, "2026-11-05T00:00:00Z", "2026-11-05T00:00:00Z", 10).is_empty());
}

/// Madrid moves its clocks forward on 2027-03-28 at 02:00: 02:30 does not exist that day,
/// so the occurrence moves to the first valid instant after it, 03:00.
#[test]
fn a_clock_time_the_change_of_hour_skips_moves_to_the_first_valid_instant() {
    let daily = series("FREQ=DAILY;COUNT=5", "2027-03-26 02:30", None, Madrid);
    assert_eq!(
        clocks(&daily, EPOCH, FAR, 10),
        [
            "2027-03-26 02:30",
            "2027-03-27 02:30",
            "2027-03-28 03:00",
            "2027-03-29 02:30",
            "2027-03-30 02:30",
        ]
    );
}

/// Madrid moves its clocks back on 2026-10-25 at 03:00: 02:30 happens twice, and the
/// occurrence takes the first of the two (the summer offset).
#[test]
fn a_clock_time_the_change_of_hour_repeats_takes_the_first_one() {
    let daily = series("FREQ=DAILY;COUNT=3", "2026-10-24 02:30", None, Madrid);
    let mut steps = MAX_STEPS;
    let found = occurrences(&daily, instant(EPOCH), instant(FAR), 10, &mut steps);
    assert_eq!(found.occurrences[1].start, instant("2026-10-25T00:30:00Z"));
    assert_eq!(found.occurrences[2].start, instant("2026-10-26T01:30:00Z"));
}

/// An all-day occurrence keeps its wall-clock span, so the day of the clock change ends at
/// 00:00 Madrid like the others, even though it is 25 hours long in UTC.
#[test]
fn an_all_day_occurrence_ends_at_the_local_midnight_after_its_day() {
    let all_day = series(
        "FREQ=DAILY;COUNT=2",
        "2026-10-24 00:00",
        Some(24 * 60),
        Madrid,
    );
    let mut steps = MAX_STEPS;
    let found = occurrences(&all_day, instant(EPOCH), instant(FAR), 10, &mut steps);
    let first = found.occurrences[0];
    assert_eq!(first.start, instant("2026-10-23T22:00:00Z"));
    assert_eq!(first.end, Some(instant("2026-10-24T22:00:00Z")));
    let second = found.occurrences[1];
    assert_eq!(second.start, instant("2026-10-24T22:00:00Z"));
    assert_eq!(second.end, Some(instant("2026-10-25T23:00:00Z")));
}

#[test]
fn an_occurrence_names_no_end_when_the_series_names_none() {
    let weekly = series("FREQ=WEEKLY;COUNT=1", "2026-11-02 09:30", None, Madrid);
    let mut steps = MAX_STEPS;
    let found = occurrences(&weekly, instant(EPOCH), instant(FAR), 1, &mut steps);
    assert_eq!(found.occurrences[0].end, None);
}
