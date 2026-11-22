//! The time of the calendar (CALENDARIO-F4): the company's IANA zone, the local clock
//! of a person's event turned into the UTC instant the store keeps, and the other way
//! round. Pure functions, so the daylight-saving rules are tested without a database.
//! The store keeps UTC; only what a person types or reads is local.

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::CalendarError;

/// The zone a name names, when it is an IANA zone (`Europe/Madrid`, `UTC`).
pub fn parse_zone(name: &str) -> Option<Tz> {
    name.parse().ok()
}

/// The UTC instant of a local clock reading in `zone`. A reading the change of hour skips
/// is refused; one it repeats is the first of the two, as the RFC decides.
pub fn local_to_utc(local: NaiveDateTime, zone: Tz) -> Result<DateTime<Utc>, CalendarError> {
    match zone.from_local_datetime(&local) {
        chrono::LocalResult::Single(instant) => Ok(instant.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(first, _) => Ok(first.with_timezone(&Utc)),
        chrono::LocalResult::None => Err(CalendarError::InvalidLocalTime),
    }
}

/// The local clock reading of an instant in `zone`.
pub fn utc_to_local(instant: DateTime<Utc>, zone: Tz) -> NaiveDateTime {
    instant.with_timezone(&zone).naive_local()
}

/// The instants where a local day starts and where the next one starts. A spring-change
/// day is 23 hours long, so the bounds are not a fixed 24 hours apart.
pub fn local_day_bounds(
    day: NaiveDate,
    zone: Tz,
) -> Result<(DateTime<Utc>, DateTime<Utc>), CalendarError> {
    let start = local_to_utc(day.and_time(NaiveTime::MIN), zone)?;
    let next = day.succ_opt().ok_or(CalendarError::InvalidLocalTime)?;
    let end = local_to_utc(next.and_time(NaiveTime::MIN), zone)?;
    Ok((start, end))
}

/// A time as a person or a tool types it. A clock reading without offset is local to the
/// company's zone; with an offset (`Z` or `+01:00`) it is that instant; a bare date is an
/// all-day start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventTime {
    Local(NaiveDateTime),
    Offset(DateTime<chrono::FixedOffset>),
    Date(NaiveDate),
}

impl EventTime {
    /// `None` when the text is none of the three forms.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(date) = parse_date(text) {
            return Some(Self::Date(date));
        }
        let zulu = text.strip_suffix('Z').map(|head| format!("{head}+00:00"));
        let text = zulu.as_deref().unwrap_or(text);
        for format in ["%Y-%m-%dT%H:%M:%S%:z", "%Y-%m-%dT%H:%M%:z"] {
            if let Ok(instant) = DateTime::parse_from_str(text, format) {
                return Some(Self::Offset(instant));
            }
        }
        for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
            if let Ok(local) = NaiveDateTime::parse_from_str(text, format) {
                return Some(Self::Local(local));
            }
        }
        None
    }

    /// The instant of a time that carries its own offset.
    pub fn fixed_instant(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Offset(instant) => Some(instant.with_timezone(&Utc)),
            Self::Local(_) | Self::Date(_) => None,
        }
    }
}

/// `YYYY-MM-DD` and nothing else: a date with a time is not an all-day start.
fn parse_date(text: &str) -> Option<NaiveDate> {
    (text.len() == 10)
        .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
        .flatten()
}

#[cfg(test)]
#[path = "timezone_tests.rs"]
mod tests;
