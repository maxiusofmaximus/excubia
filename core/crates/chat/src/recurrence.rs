//! The recurrence of a calendar event (CALENDARIO-F4b): a rule in the RFC 5545 text form,
//! restricted to the subset the calendar keeps. The rule is normalized on the way in, so the
//! stored text has one spelling and reads back to the same rule. Pure: no database, no clock.

use std::fmt;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc, Weekday};
use thiserror::Error;

/// The most a `COUNT` may name.
pub const COUNT_MAX: u32 = 1_000;
/// The largest `INTERVAL`.
pub const INTERVAL_MAX: u32 = 999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// The last moment of a series: a UTC instant, or a local date that includes its whole day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    Instant(DateTime<Utc>),
    Date(NaiveDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Forever,
    Count(u32),
    Until(Until),
}

/// A recurrence rule. `byday` is sorted and without repeats; it is empty unless `freq` is
/// weekly, and an empty one means the weekday of the start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: u32,
    pub byday: Vec<Weekday>,
    pub end: End,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RecurrenceError {
    /// A part RFC 5545 defines but the calendar does not keep, such as `BYMONTHDAY`.
    #[error("{0} is not supported: the calendar keeps FREQ, INTERVAL, BYDAY (weekly only), COUNT and UNTIL")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
}

impl Rule {
    /// The rule a text names. The `RRULE:` prefix is optional, and names are not case-sensitive.
    pub fn parse(text: &str) -> Result<Self, RecurrenceError> {
        let text = text.trim();
        let body = match text.get(..6) {
            Some(prefix) if prefix.eq_ignore_ascii_case("RRULE:") => &text[6..],
            _ => text,
        };
        let mut parts = Parts::default();
        for part in body.split(';') {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| invalid(format!("{part:?} is not KEY=VALUE")))?;
            parts.set(
                &key.trim().to_ascii_uppercase(),
                &value.trim().to_ascii_uppercase(),
            )?;
        }
        parts.build()
    }
}

impl Freq {
    fn name(self) -> &'static str {
        match self {
            Self::Daily => "DAILY",
            Self::Weekly => "WEEKLY",
            Self::Monthly => "MONTHLY",
            Self::Yearly => "YEARLY",
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FREQ={};INTERVAL={}", self.freq.name(), self.interval)?;
        if !self.byday.is_empty() {
            let codes: Vec<&str> = self.byday.iter().map(|day| day_code(*day)).collect();
            write!(f, ";BYDAY={}", codes.join(","))?;
        }
        match self.end {
            End::Forever => Ok(()),
            End::Count(count) => write!(f, ";COUNT={count}"),
            End::Until(Until::Instant(at)) => write!(f, ";UNTIL={}", at.format("%Y%m%dT%H%M%SZ")),
            End::Until(Until::Date(day)) => write!(f, ";UNTIL={}", day.format("%Y%m%d")),
        }
    }
}

/// The parts of a rule as they are read, before the subset's own rules are checked.
#[derive(Default)]
struct Parts {
    freq: Option<Freq>,
    interval: Option<u32>,
    byday: Option<Vec<Weekday>>,
    count: Option<u32>,
    until: Option<Until>,
}

impl Parts {
    fn set(&mut self, key: &str, value: &str) -> Result<(), RecurrenceError> {
        match key {
            "FREQ" => once(&mut self.freq, key, freq(value)?),
            "INTERVAL" => once(&mut self.interval, key, number(key, value, INTERVAL_MAX)?),
            "BYDAY" => once(&mut self.byday, key, byday(value)?),
            "COUNT" => once(&mut self.count, key, number(key, value, COUNT_MAX)?),
            "UNTIL" => once(&mut self.until, key, until(value)?),
            other => Err(RecurrenceError::Unsupported(other.to_string())),
        }
    }

    fn build(self) -> Result<Rule, RecurrenceError> {
        let freq = self
            .freq
            .ok_or_else(|| invalid("FREQ is required".to_string()))?;
        if self.byday.is_some() && freq != Freq::Weekly {
            return Err(RecurrenceError::Unsupported(
                "BYDAY outside FREQ=WEEKLY".to_string(),
            ));
        }
        let end = match (self.count, self.until) {
            (Some(_), Some(_)) => {
                return Err(invalid("COUNT and UNTIL cannot both appear".to_string()))
            }
            (Some(count), None) => End::Count(count),
            (None, Some(until)) => End::Until(until),
            (None, None) => End::Forever,
        };
        Ok(Rule {
            freq,
            interval: self.interval.unwrap_or(1),
            byday: self.byday.unwrap_or_default(),
            end,
        })
    }
}

fn once<T>(slot: &mut Option<T>, key: &str, value: T) -> Result<(), RecurrenceError> {
    if slot.is_some() {
        return Err(invalid(format!("{key} appears twice")));
    }
    *slot = Some(value);
    Ok(())
}

fn invalid(reason: String) -> RecurrenceError {
    RecurrenceError::Invalid(reason)
}

fn freq(value: &str) -> Result<Freq, RecurrenceError> {
    match value {
        "DAILY" => Ok(Freq::Daily),
        "WEEKLY" => Ok(Freq::Weekly),
        "MONTHLY" => Ok(Freq::Monthly),
        "YEARLY" => Ok(Freq::Yearly),
        "SECONDLY" | "MINUTELY" | "HOURLY" => {
            Err(RecurrenceError::Unsupported(format!("FREQ={value}")))
        }
        _ => Err(invalid(format!("FREQ={value} is not a frequency"))),
    }
}

fn number(key: &str, value: &str, max: u32) -> Result<u32, RecurrenceError> {
    value
        .parse::<u32>()
        .ok()
        .filter(|number| (1..=max).contains(number))
        .ok_or_else(|| invalid(format!("{key} must be 1 to {max}")))
}

/// The days of a `BYDAY`, sorted from Monday and without repeats. An ordinal such as `1MO`
/// is RFC 5545 but outside the subset.
fn byday(value: &str) -> Result<Vec<Weekday>, RecurrenceError> {
    let mut days = Vec::new();
    for token in value.split(',') {
        match weekday(token) {
            Some(day) => days.push(day),
            None if token.len() > 2 && token.get(token.len() - 2..).and_then(weekday).is_some() => {
                return Err(RecurrenceError::Unsupported(format!("BYDAY={token}")));
            }
            None => return Err(invalid(format!("BYDAY={token} is not a weekday"))),
        }
    }
    days.sort_by_key(|day| day.num_days_from_monday());
    days.dedup();
    Ok(days)
}

/// `20301231T235959Z` (a UTC instant) or `20301231` (a local date). Anything else is refused.
fn until(value: &str) -> Result<Until, RecurrenceError> {
    if let Some(clock) = value.strip_suffix('Z') {
        return NaiveDateTime::parse_from_str(clock, "%Y%m%dT%H%M%S")
            .map(|at| Until::Instant(at.and_utc()))
            .map_err(|_| {
                invalid(format!(
                    "UNTIL={value} is not a UTC time such as 20301231T235959Z"
                ))
            });
    }
    NaiveDate::parse_from_str(value, "%Y%m%d")
        .map(Until::Date)
        .map_err(|_| invalid(format!("UNTIL={value} is not a date such as 20301231")))
}

const WEEKDAYS: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

fn day_code(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

fn weekday(code: &str) -> Option<Weekday> {
    WEEKDAYS.into_iter().find(|day| day_code(*day) == code)
}

#[cfg(test)]
#[path = "recurrence_tests.rs"]
mod tests;
