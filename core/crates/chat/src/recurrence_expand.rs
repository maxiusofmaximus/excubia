//! The occurrences of a recurring event (CALENDARIO-F4b): the local clock of each period the
//! rule names, read in the company's zone. Pure: the store feeds it a series and a window, and
//! it never reads the database or the clock.
//!
//! Three rules settle what the clock does. A MONTHLY or YEARLY occurrence on a day the month
//! lacks (the 30th of February) is skipped, as RFC 5545 has it. A clock time the change of hour
//! skips moves to the first valid instant after it. One the change repeats is the first of the two.

use chrono::{
    DateTime, Datelike, Days, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc, Weekday,
};
use chrono_tz::Tz;

use crate::recurrence::{End, Freq, Rule, Until};
use crate::timezone::utc_to_local;

/// The periods one query walks. A window far from the start is reached by a jump, so this
/// bounds the walk through a window or through a counted series.
pub const MAX_STEPS: usize = 10_000;

/// Minutes to look past a clock time the change of hour skips; the longest change is an hour.
const GAP_MINUTES_MAX: i64 = 180;

/// A series: its rule, the local clock of its first occurrence, the wall-clock span of one
/// occurrence (`None` when the event has no end) and the zone the clock is read in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    pub rule: Rule,
    pub first: NaiveDateTime,
    pub span: Option<Duration>,
    pub zone: Tz,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
}

pub struct Expansion {
    pub occurrences: Vec<Occurrence>,
    /// `true` when the step budget ran out before the window was walked.
    pub exhausted: bool,
}

/// What one candidate day of the walk comes to.
enum Next {
    Keep(Occurrence),
    Skip,
    Stop,
}

/// The occurrences that start in `[from, to)`, earliest first, at most `max` of them. `steps`
/// is the budget a caller shares across the series of one query; each period walked spends one.
pub fn occurrences(
    series: &Series,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    max: usize,
    steps: &mut usize,
) -> Expansion {
    let mut found = Vec::new();
    let mut counted = 0;
    let mut period = first_period(series, from);
    let exhausted = 'walk: loop {
        if found.len() >= max {
            break false;
        }
        if *steps == 0 {
            break true;
        }
        *steps -= 1;
        for day in period_days(series, period) {
            if found.len() >= max {
                break 'walk false;
            }
            match judge(series, day, &mut counted, from, to) {
                Next::Keep(occurrence) => found.push(occurrence),
                Next::Skip => {}
                Next::Stop => break 'walk false,
            }
        }
        period += 1;
    };
    Expansion {
        occurrences: found,
        exhausted,
    }
}

/// Whether one candidate day is an occurrence. COUNT counts every occurrence since the start,
/// including the ones before `from`, so a counted series is always walked from its start.
fn judge(
    series: &Series,
    day: NaiveDate,
    counted: &mut u32,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Next {
    let wall = day.and_time(series.first.time());
    if wall < series.first {
        return Next::Skip;
    }
    let Some(start) = wall_instant(series.zone, wall) else {
        return Next::Skip;
    };
    *counted += 1;
    match series.rule.end {
        End::Count(count) if *counted > count => return Next::Stop,
        End::Until(Until::Date(last)) if day > last => return Next::Stop,
        End::Until(Until::Instant(last)) if start > last => return Next::Stop,
        _ => {}
    }
    if start >= to {
        return Next::Stop;
    }
    if start < from {
        return Next::Skip;
    }
    let end = series
        .span
        .and_then(|span| wall_instant(series.zone, wall + span));
    Next::Keep(Occurrence { start, end })
}

/// The first period that can hold an occurrence at or after `from`. A counted series starts at
/// period 0: COUNT needs every occurrence since the start. Otherwise the period is estimated
/// from the calendar, one period early to be safe.
fn first_period(series: &Series, from: DateTime<Utc>) -> u64 {
    if matches!(series.rule.end, End::Count(_)) {
        return 0;
    }
    let first = series.first.date();
    let from = utc_to_local(from, series.zone).date();
    let elapsed = match series.rule.freq {
        Freq::Daily => (from - first).num_days(),
        Freq::Weekly => (monday(from) - monday(first)).num_days() / 7,
        Freq::Monthly => month_index(from) - month_index(first),
        Freq::Yearly => i64::from(from.year() - first.year()),
    };
    let periods = u64::try_from(elapsed).unwrap_or(0) / u64::from(series.rule.interval);
    periods.saturating_sub(1)
}

/// The local days a period holds. A period is `interval` units of the frequency long, so
/// period `k` starts `k * interval` days, weeks, months or years after the first day.
fn period_days(series: &Series, period: u64) -> Vec<NaiveDate> {
    let rule = &series.rule;
    let first = series.first.date();
    let units = period.saturating_mul(u64::from(rule.interval));
    match rule.freq {
        Freq::Daily => first
            .checked_add_days(Days::new(units))
            .into_iter()
            .collect(),
        Freq::Weekly => {
            let week = monday(first).checked_add_days(Days::new(units.saturating_mul(7)));
            let fallback = [first.weekday()];
            let days: &[Weekday] = if rule.byday.is_empty() {
                &fallback
            } else {
                &rule.byday
            };
            week.into_iter()
                .flat_map(|week| {
                    days.iter().map(move |day| {
                        week + Duration::days(i64::from(day.num_days_from_monday()))
                    })
                })
                .collect()
        }
        Freq::Monthly => monthly_day(first, units).into_iter().collect(),
        Freq::Yearly => yearly_day(first, units).into_iter().collect(),
    }
}

fn monthly_day(first: NaiveDate, months: u64) -> Option<NaiveDate> {
    let total = month_index(first).checked_add(i64::try_from(months).ok()?)?;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = u32::try_from(total.rem_euclid(12)).ok()? + 1;
    NaiveDate::from_ymd_opt(year, month, first.day())
}

fn yearly_day(first: NaiveDate, years: u64) -> Option<NaiveDate> {
    let year = first.year().checked_add(i32::try_from(years).ok()?)?;
    NaiveDate::from_ymd_opt(year, first.month(), first.day())
}

/// The instant a local clock reading names. A reading the change of hour skips moves to the
/// first valid instant after it; one the change repeats is the first of the two.
pub fn wall_instant(zone: Tz, wall: NaiveDateTime) -> Option<DateTime<Utc>> {
    (0..=GAP_MINUTES_MAX).find_map(|minutes| {
        match zone.from_local_datetime(&(wall + Duration::minutes(minutes))) {
            chrono::LocalResult::Single(at) | chrono::LocalResult::Ambiguous(at, _) => {
                Some(at.with_timezone(&Utc))
            }
            chrono::LocalResult::None => None,
        }
    })
}

fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

fn month_index(day: NaiveDate) -> i64 {
    i64::from(day.year()) * 12 + i64::from(day.month0())
}

#[cfg(test)]
#[path = "recurrence_expand_tests.rs"]
mod tests;
