//! When a schedule is due (F4-CORE). The cases the clock cannot get wrong: the next run
//! after an instant, a run the change of hour moves, a weekly rule with days, a one-time
//! run, and a run that is on time or late.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::Europe::Madrid;

use crate::model_schedule::{Cadence, Timing, Verdict};
use crate::recurrence::{End, Freq, Rule};
use crate::schedule_due::{next_after, verdict};

fn at(text: &str) -> DateTime<Utc> {
    text.parse().expect("an RFC 3339 instant")
}

fn daily(start: &str, time: &str) -> Timing {
    Timing {
        cadence: Cadence::Repeat(Rule {
            freq: Freq::Daily,
            interval: 1,
            byday: Vec::new(),
            end: End::Forever,
        }),
        start_date: NaiveDate::parse_from_str(start, "%Y-%m-%d").expect("date"),
        time: NaiveTime::parse_from_str(time, "%H:%M").expect("time"),
        zone: Madrid,
    }
}

#[test]
fn a_daily_run_is_the_next_local_nine_after_the_instant() {
    // 09:00 in Madrid after the October change is 08:00 UTC.
    let timing = daily("2026-11-02", "09:00");
    assert_eq!(
        next_after(&timing, at("2026-11-04T07:59:00Z")),
        Some(at("2026-11-04T08:00:00Z"))
    );
}

#[test]
fn the_run_at_the_instant_itself_is_not_the_next_one() {
    let timing = daily("2026-11-02", "09:00");
    assert_eq!(
        next_after(&timing, at("2026-11-04T08:00:00Z")),
        Some(at("2026-11-05T08:00:00Z"))
    );
}

#[test]
fn a_weekly_rule_with_days_skips_to_the_next_named_day() {
    let timing = Timing {
        cadence: Cadence::Repeat(Rule {
            freq: Freq::Weekly,
            interval: 1,
            byday: vec![chrono::Weekday::Mon, chrono::Weekday::Wed],
            end: End::Forever,
        }),
        start_date: NaiveDate::from_ymd_opt(2026, 11, 2).expect("date"),
        time: NaiveTime::from_hms_opt(9, 0, 0).expect("time"),
        zone: Madrid,
    };
    // Monday 2 November at 09:00 has passed; Wednesday 4 November is next.
    assert_eq!(
        next_after(&timing, at("2026-11-02T08:30:00Z")),
        Some(at("2026-11-04T08:00:00Z"))
    );
}

#[test]
fn a_one_time_run_has_one_instant_and_then_none() {
    let timing = Timing {
        cadence: Cadence::Once,
        start_date: NaiveDate::from_ymd_opt(2026, 11, 10).expect("date"),
        time: NaiveTime::from_hms_opt(9, 0, 0).expect("time"),
        zone: Madrid,
    };
    assert_eq!(
        next_after(&timing, at("2026-11-09T12:00:00Z")),
        Some(at("2026-11-10T08:00:00Z"))
    );
    assert_eq!(next_after(&timing, at("2026-11-10T08:00:00Z")), None);
}

#[test]
fn a_clock_time_the_spring_change_skips_moves_to_the_first_valid_instant() {
    // 02:30 does not exist on 28 March 2027 in Madrid: the run moves to 03:00 CEST (01:00 UTC).
    let timing = daily("2027-03-27", "02:30");
    assert_eq!(
        next_after(&timing, at("2027-03-27T12:00:00Z")),
        Some(at("2027-03-28T01:00:00Z"))
    );
    // The day after, 02:30 exists again: 02:30 CEST is 00:30 UTC.
    assert_eq!(
        next_after(&timing, at("2027-03-28T01:00:00Z")),
        Some(at("2027-03-29T00:30:00Z"))
    );
}

#[test]
fn a_run_on_time_is_launched_and_one_two_minutes_late_is_missed() {
    let due = at("2026-11-04T08:00:00Z");
    assert_eq!(verdict(due, at("2026-11-04T08:00:00Z")), Verdict::Launch);
    assert_eq!(verdict(due, at("2026-11-04T08:01:59Z")), Verdict::Launch);
    assert_eq!(verdict(due, at("2026-11-04T08:02:00Z")), Verdict::Missed);
}

#[test]
fn a_run_of_a_host_that_was_off_for_days_is_missed_not_launched() {
    let due = at("2026-11-04T08:00:00Z");
    assert_eq!(verdict(due, at("2026-11-07T10:00:00Z")), Verdict::Missed);
}
