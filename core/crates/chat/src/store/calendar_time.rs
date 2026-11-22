//! How a typed time becomes the instants an event keeps (CALENDARIO-F4). One rule serves the
//! create and the edit: an all-day event spans whole local days, a timed one spans two
//! instants, and a time that carries no offset is read in the zone it is given.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

use crate::error::{CalendarError, ChatError};
use crate::model_calendar::CalendarEvent;
use crate::timezone::{local_day_bounds, local_to_utc, parse_zone, utc_to_local, EventTime};

/// The instants an event spans. An all-day event always has an end.
pub(super) struct Times {
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
}

pub(super) fn zone_of(name: &str) -> Result<Tz, CalendarError> {
    parse_zone(name).ok_or(CalendarError::InvalidZone)
}

/// The UTC text the store keeps, `%Y-%m-%dT%H:%M:%S%.3fZ`, which sorts as a string.
pub(super) fn instant_text(instant: DateTime<Utc>) -> String {
    instant.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub(super) fn instant_of(text: &str) -> Result<DateTime<Utc>, CalendarError> {
    DateTime::parse_from_rfc3339(text)
        .map(|instant| instant.with_timezone(&Utc))
        .map_err(|_| ChatError::Inconsistent("calendar instant").into())
}

pub(super) fn resolve(
    zone: Tz,
    all_day: bool,
    start: EventTime,
    end: Option<EventTime>,
) -> Result<Times, CalendarError> {
    if all_day {
        resolve_all_day(zone, start, end)
    } else {
        resolve_timed(zone, start, end)
    }
}

/// An all-day start is a date; its end, when named, is the last day it covers. Without one
/// the event covers its start day alone.
fn resolve_all_day(
    zone: Tz,
    start: EventTime,
    end: Option<EventTime>,
) -> Result<Times, CalendarError> {
    let EventTime::Date(first) = start else {
        return Err(invalid("start", "an all-day event starts on a date"));
    };
    let (start, next_day) = local_day_bounds(first, zone)?;
    let end = match end {
        None => next_day,
        Some(EventTime::Date(last)) if last >= first => local_day_bounds(last, zone)?.1,
        Some(EventTime::Date(_)) => {
            return Err(invalid("end", "the last day is not before the first"))
        }
        Some(_) => return Err(invalid("end", "an all-day event ends on a date")),
    };
    Ok(Times {
        start,
        end: Some(end),
    })
}

fn resolve_timed(
    zone: Tz,
    start: EventTime,
    end: Option<EventTime>,
) -> Result<Times, CalendarError> {
    let start = instant(zone, start, "start")?;
    let end = end.map(|end| instant(zone, end, "end")).transpose()?;
    if end.is_some_and(|end| end <= start) {
        return Err(invalid("end", "an event ends after it starts"));
    }
    Ok(Times { start, end })
}

fn instant(zone: Tz, time: EventTime, field: &'static str) -> Result<DateTime<Utc>, CalendarError> {
    match time {
        EventTime::Local(local) => local_to_utc(local, zone),
        EventTime::Offset(instant) => Ok(instant.with_timezone(&Utc)),
        EventTime::Date(_) => Err(invalid(field, "a timed event needs a clock time")),
    }
}

/// The start and end of a stored event as the times an edit starts from, read in its zone.
/// An all-day end comes back as its last covered day, so the same rule resolves it again.
pub(super) fn current_times(
    event: &CalendarEvent,
    zone: Tz,
) -> Result<(EventTime, Option<EventTime>), CalendarError> {
    let start = instant_of(&event.start_utc)?;
    let end = event.end_utc.as_deref().map(instant_of).transpose()?;
    if event.all_day {
        let day = utc_to_local(start, zone).date();
        let last = end
            .map(|end| utc_to_local(end, zone).date())
            .map(|next: NaiveDate| next - Duration::days(1))
            .unwrap_or(day);
        return Ok((EventTime::Date(day), Some(EventTime::Date(last))));
    }
    Ok((
        EventTime::Local(utc_to_local(start, zone)),
        end.map(|end| EventTime::Local(utc_to_local(end, zone))),
    ))
}

fn invalid(field: &'static str, message: &'static str) -> CalendarError {
    CalendarError::InvalidField { field, message }
}
