//! The store's side of recurring events (CALENDARIO-F4b): a stored series read as the
//! expander's [`Series`], the check a rule needs against its start, and the occurrence a
//! reminder is due for. The rule is parsed in `recurrence`, the dates are worked out in
//! `recurrence_expand`; this file only reads the rows.

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

use super::calendar::{event_row, EVENT_SELECT};
use super::calendar_time::{instant_of, zone_of};
use crate::error::{CalendarError, ChatError};
use crate::model_calendar::CalendarEvent;
use crate::recurrence::{End, Rule, Until};
use crate::recurrence_expand::{occurrences, Series, MAX_STEPS};
use crate::timezone::utc_to_local;

/// The instant a clock reading names.
pub(super) fn instant_at(now_ms: i64) -> Result<DateTime<Utc>, CalendarError> {
    DateTime::<Utc>::from_timestamp_millis(now_ms).ok_or(ChatError::Inconsistent("clock").into())
}

/// The series a stored event is, `None` for a single event.
pub(super) fn series_of(event: &CalendarEvent) -> Result<Option<Series>, CalendarError> {
    let Some(text) = event.recurrence.as_deref() else {
        return Ok(None);
    };
    let rule = Rule::parse(text).map_err(|_| ChatError::Inconsistent("calendar recurrence"))?;
    let zone = zone_of(&event.timezone)?;
    let first = utc_to_local(instant_of(&event.start_utc)?, zone);
    let span = match &event.end_utc {
        Some(end) => Some(utc_to_local(instant_of(end)?, zone) - first),
        None => None,
    };
    Ok(Some(Series {
        rule,
        first,
        span,
        zone,
    }))
}

/// Refuses a series whose UNTIL falls before its first occurrence: it would never occur.
pub(super) fn check_series(event: &CalendarEvent) -> Result<(), CalendarError> {
    let Some(series) = series_of(event)? else {
        return Ok(());
    };
    let start = instant_of(&event.start_utc)?;
    let before = match series.rule.end {
        End::Until(Until::Instant(last)) => last < start,
        End::Until(Until::Date(last)) => last < series.first.date(),
        End::Forever | End::Count(_) => false,
    };
    match before {
        true => Err(CalendarError::InvalidField {
            field: "recurrence",
            message: "UNTIL falls before the first occurrence",
        }),
        false => Ok(()),
    }
}

/// The occurrence a reminder is due for at `now`: the first one after the last reminded, not
/// earlier than the catch-up window. Occurrences of one series are a day apart at least, so a
/// window holds only a few of them.
pub(super) fn due_occurrence(
    event: &CalendarEvent,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, CalendarError> {
    let (Some(series), Some(minutes)) = (series_of(event)?, event.reminder_minutes) else {
        return Ok(None);
    };
    let reminder = Duration::minutes(minutes);
    let floor = cutoff + reminder;
    let from = match &event.reminded_occurrence {
        Some(last) => (instant_of(last)? + Duration::milliseconds(1)).max(floor),
        None => floor,
    };
    let mut steps = MAX_STEPS;
    let found = occurrences(
        &series,
        from,
        now + reminder + Duration::milliseconds(1),
        1,
        &mut steps,
    );
    Ok(found.occurrences.first().map(|occurrence| occurrence.start))
}

/// Every recurring event. A window or a reminder asks each one for its occurrences; a series
/// with no occurrence in the range answers with none.
pub(super) fn recurring_events(
    connection: &Connection,
) -> Result<Vec<CalendarEvent>, CalendarError> {
    let mut statement =
        connection.prepare(&format!("{EVENT_SELECT} WHERE recurrence IS NOT NULL"))?;
    let events = statement.query_map([], event_row)?;
    Ok(events.collect::<Result<_, _>>()?)
}
