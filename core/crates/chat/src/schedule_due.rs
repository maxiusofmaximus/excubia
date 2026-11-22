//! When a schedule is due (F4-CORE): the first run after an instant, and whether a due run
//! is still launched or is already too late. Pure: no database, no clock. The store passes the
//! instants, so the change of hour and the day count are settled here, once.

use chrono::{DateTime, Duration, Utc};

use crate::model_schedule::{Cadence, Timing, Verdict, LATE_LIMIT_SECS};
use crate::recurrence::Rule;
use crate::recurrence_expand::{occurrences, wall_instant, Series, MAX_STEPS};

#[cfg(test)]
#[path = "schedule_due_tests.rs"]
mod tests;

/// How far past an instant the rule is searched: nine years reaches a yearly rule's next
/// day that exists, and the monthly and weekly rules always find theirs well before it.
const SEARCH_DAYS: i64 = 9 * 366;

/// The first run strictly after `after`, or `None` when no run follows (a one-time run that
/// has passed). A rule is walked from its start, so the count of a series is honoured.
pub fn next_after(timing: &Timing, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let first = timing.start_date.and_time(timing.time);
    match &timing.cadence {
        Cadence::Once => wall_instant(timing.zone, first).filter(|at| *at > after),
        Cadence::Repeat(rule) => repeat_after(rule, timing, first, after),
    }
}

fn repeat_after(
    rule: &Rule,
    timing: &Timing,
    first: chrono::NaiveDateTime,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let series = Series {
        rule: rule.clone(),
        first,
        span: None,
        zone: timing.zone,
    };
    let from = after + Duration::milliseconds(1);
    let to = from + Duration::days(SEARCH_DAYS);
    let mut steps = MAX_STEPS;
    occurrences(&series, from, to, 1, &mut steps)
        .occurrences
        .first()
        .map(|occurrence| occurrence.start)
}

/// A run due at `due` is launched when the host reaches it within the late limit, and missed
/// otherwise. A run seen before it is due is not missed: it is launched.
pub fn verdict(due: DateTime<Utc>, now: DateTime<Utc>) -> Verdict {
    if now - due < Duration::seconds(LATE_LIMIT_SECS) {
        Verdict::Launch
    } else {
        Verdict::Missed
    }
}
