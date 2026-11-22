//! The times of a file: a `DTSTART` or `DTEND` value, a `DURATION`, and a `VALARM` trigger
//! read as minutes before the start. Each reads only the forms the calendar keeps.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

use super::reader::Property;
use super::SkipReason;
use crate::model_calendar::REMINDER_MAX_MINUTES;
use crate::timezone::{local_to_utc, parse_zone};

/// A start or an end as the file wrote it, before it is tied to the company's zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stamp {
    Date(NaiveDate),
    /// A clock with no zone: local to the company.
    Floating(NaiveDateTime),
    Utc(DateTime<Utc>),
}

/// A `DTSTART` or `DTEND` property. A `TZID` clock is converted here, with the offset its day has.
pub fn stamp(prop: &Property) -> Result<Stamp, SkipReason> {
    let value = prop.value.as_str();
    if prop.param("VALUE") == Some("DATE") {
        return date_of(value)
            .map(Stamp::Date)
            .ok_or(SkipReason::InvalidTime);
    }
    let (day, clock) = value.split_once('T').ok_or(SkipReason::InvalidTime)?;
    let (clock, zulu) = match clock.strip_suffix('Z') {
        Some(clock) => (clock, true),
        None => (clock, false),
    };
    let naive = NaiveDateTime::new(
        date_of(day).ok_or(SkipReason::InvalidTime)?,
        clock_of(clock)?,
    );
    if zulu {
        return Ok(Stamp::Utc(Utc.from_utc_datetime(&naive)));
    }
    let Some(name) = prop.param("TZID") else {
        return Ok(Stamp::Floating(naive));
    };
    let zone = parse_zone(name).ok_or(SkipReason::InvalidTime)?;
    local_to_utc(naive, zone)
        .map(Stamp::Utc)
        .map_err(|_| SkipReason::InvalidTime)
}

/// A `DURATION` value such as `PT1H30M`, `P1DT2H`, `P2W`, with an optional sign.
pub fn duration(text: &str) -> Option<Duration> {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let seconds = duration_seconds(rest.strip_prefix('P')?)?;
    Duration::try_seconds(sign * seconds)
}

/// The minutes a `VALARM` trigger sets before the start: `-PT15M` is 15. Only a whole number
/// of minutes, at most a week, before the start is kept.
pub fn trigger_minutes(text: &str) -> Option<i64> {
    let body = text.strip_prefix('-')?;
    if body.starts_with(['+', '-']) {
        return None;
    }
    let seconds = duration(body)?.num_seconds();
    let minutes = seconds / 60;
    (seconds % 60 == 0 && (0..=REMINDER_MAX_MINUTES).contains(&minutes)).then_some(minutes)
}

fn duration_seconds(body: &str) -> Option<i64> {
    if let Some(weeks) = body.strip_suffix('W') {
        return number(weeks)?.checked_mul(WEEK_SECONDS);
    }
    let (days, clock) = match body.split_once('T') {
        Some((days, clock)) if !clock.is_empty() => (days, clock),
        Some(_) => return None,
        None => (body, ""),
    };
    if days.is_empty() && clock.is_empty() {
        return None;
    }
    units(days, &[('D', DAY_SECONDS)])?
        .checked_add(units(clock, &[('H', 3_600), ('M', 60), ('S', 1)])?)
}

const DAY_SECONDS: i64 = 86_400;
const WEEK_SECONDS: i64 = 7 * DAY_SECONDS;

/// `2H30M`-style groups, each unit once and in the table's order.
fn units(text: &str, table: &[(char, i64)]) -> Option<i64> {
    let mut total = 0_i64;
    let mut rest = text;
    let mut next = 0;
    while !rest.is_empty() {
        let end = rest.find(|ch: char| !ch.is_ascii_digit())?;
        let count = number(&rest[..end])?;
        let unit = rest[end..].chars().next()?;
        let index = table.iter().position(|(letter, _)| *letter == unit)?;
        if index < next {
            return None;
        }
        next = index + 1;
        total = total.checked_add(count.checked_mul(table[index].1)?)?;
        rest = &rest[end + unit.len_utf8()..];
    }
    Some(total)
}

fn number(text: &str) -> Option<i64> {
    digits(text).then(|| text.parse().ok()).flatten()
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `YYYYMMDD`.
fn date_of(text: &str) -> Option<NaiveDate> {
    if text.len() != 8 || !digits(text) {
        return None;
    }
    NaiveDate::from_ymd_opt(
        text[..4].parse().ok()?,
        text[4..6].parse().ok()?,
        text[6..].parse().ok()?,
    )
}

/// `HHMMSS`.
fn clock_of(text: &str) -> Result<NaiveTime, SkipReason> {
    if text.len() != 6 || !digits(text) {
        return Err(SkipReason::InvalidTime);
    }
    NaiveTime::from_hms_opt(
        text[..2].parse().map_err(|_| SkipReason::InvalidTime)?,
        text[2..4].parse().map_err(|_| SkipReason::InvalidTime)?,
        text[4..].parse().map_err(|_| SkipReason::InvalidTime)?,
    )
    .ok_or(SkipReason::InvalidTime)
}
