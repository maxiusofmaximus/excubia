//! The `.ics` exchange in the store (CALENDARIO-F4c): an import keeps the events of a file in
//! one transaction, and an export reads the events a window holds. The file's text is read and
//! written in `ics`; this file decides what the calendar keeps and why it leaves one out.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Transaction};

use super::agenda::Writer;
use super::calendar::{company_zone_name, event_row, insert_event, new_event, EVENT_SELECT};
use super::calendar_read::window_instant;
use super::calendar_series::{check_series, instant_at, series_of};
use super::calendar_time::{instant_of, zone_of};
use crate::error::CalendarError;
use crate::ics::{Draft, ImportReport, Parsed, SkipReason, HOST_UID_SUFFIX, MAX_EXPORT_EVENTS};
use crate::model_calendar::CalendarEvent;
use crate::recurrence_expand::{occurrences, MAX_STEPS};
use crate::timezone::EventTime;

impl super::ChatStore {
    /// Keeps the events of a file the app sent. An event that cannot be kept is named in the
    /// report and the others still come in; the whole file is one transaction.
    pub fn import_events(
        &self,
        device_id: &str,
        parsed: Vec<Parsed>,
    ) -> Result<ImportReport, CalendarError> {
        let now = self.now_text();
        let now_instant = instant_at(self.now_ms())?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let zone_name = company_zone_name(&transaction)?.ok_or(CalendarError::TimezoneUnset)?;
        let writer = Writer::import(device_id);
        let mut report = ImportReport::default();
        for Parsed { index, outcome } in parsed {
            let skipped = match outcome {
                Ok(draft) => keep(&transaction, &zone_name, &writer, draft, now_instant, &now)?,
                Err(reason) => Some(reason),
            };
            match skipped {
                Some(reason) => report.skip(index, reason),
                None => report.imported += 1,
            }
        }
        transaction.commit()?;
        Ok(report)
    }

    /// The events an export carries: a plain event that starts in the window and a series with
    /// an occurrence in it, each once and with its rule. Without bounds, every event. An export
    /// that would hold more than `MAX_EXPORT_EVENTS` is refused.
    pub fn export_events(
        &self,
        from: Option<EventTime>,
        to: Option<EventTime>,
    ) -> Result<Vec<CalendarEvent>, CalendarError> {
        let connection = self.lock();
        let zone_name = company_zone_name(&connection)?.ok_or(CalendarError::TimezoneUnset)?;
        let zone = zone_of(&zone_name)?;
        let from = from.map(|time| window_instant(time, zone)).transpose()?;
        let to = to.map(|time| window_instant(time, zone)).transpose()?;
        if from.zip(to).is_some_and(|(from, to)| to <= from) {
            return Err(CalendarError::InvalidField {
                field: "to",
                message: "the window ends after it starts",
            });
        }
        let mut events = Vec::new();
        for event in all_events(&connection)? {
            if in_window(&event, from, to)? {
                events.push(event);
            }
            if events.len() > MAX_EXPORT_EVENTS {
                return Err(CalendarError::TooManyEvents);
            }
        }
        Ok(events)
    }
}

/// Writes one draft unless its UID is taken. `None` when it is written; otherwise why not.
/// A rejected event is a reason; a database error is not, and ends the import.
fn keep(
    transaction: &Transaction<'_>,
    zone_name: &str,
    writer: &Writer<'_>,
    draft: Draft,
    now_instant: DateTime<Utc>,
    now: &str,
) -> Result<Option<SkipReason>, CalendarError> {
    if uid_taken(transaction, &draft.uid)? {
        return Ok(Some(SkipReason::Exists));
    }
    let mut event = match new_event(zone_name, draft.event, writer, None, Some(draft.uid), now) {
        Ok(event) => event,
        Err(error) => return skip_of(error).map(Some),
    };
    settle_reminder(&mut event, now_instant, now)?;
    let written =
        check_series(&event).and_then(|()| insert_event(transaction, *writer, &event, now));
    match written {
        Ok(()) => Ok(None),
        Err(error) => skip_of(error).map(Some),
    }
}

/// Whether the company keeps the UID already: an event that carries it, or the host event an
/// export of this company named it after.
fn uid_taken(transaction: &Transaction<'_>, uid: &str) -> Result<bool, CalendarError> {
    let host_id = uid.strip_suffix(HOST_UID_SUFFIX);
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM calendar_event WHERE uid = ?1 OR id = ?2",
        params![uid, host_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// An event that started already does not remind: its reminder is marked as sent now. A series
/// marks its last reminded occurrence, so only the ones after the import remind.
fn settle_reminder(
    event: &mut CalendarEvent,
    now_instant: DateTime<Utc>,
    now: &str,
) -> Result<(), CalendarError> {
    if instant_of(&event.start_utc)? > now_instant {
        return Ok(());
    }
    match event.recurrence {
        Some(_) => event.reminded_occurrence = Some(now.to_string()),
        None => event.reminded_at = Some(now.to_string()),
    }
    Ok(())
}

/// The reason a write that a file asked for could not be kept for. A limit or a time the calendar
/// refuses is the event's own; anything else is the store's and stays an error.
fn skip_of(error: CalendarError) -> Result<SkipReason, CalendarError> {
    match error {
        CalendarError::EventLimit => Ok(SkipReason::CalendarLimit),
        CalendarError::InvalidField {
            field: "recurrence",
            ..
        } => Ok(SkipReason::InvalidRecurrence),
        CalendarError::InvalidField { .. } | CalendarError::InvalidLocalTime => {
            Ok(SkipReason::InvalidTime)
        }
        other => Err(other),
    }
}

fn all_events(connection: &Connection) -> Result<Vec<CalendarEvent>, CalendarError> {
    let mut statement = connection.prepare(&format!("{EVENT_SELECT} ORDER BY start_utc, id"))?;
    let events = statement.query_map([], event_row)?;
    Ok(events.collect::<Result<_, _>>()?)
}

/// Whether an event belongs in an export of the window. A missing bound is open.
fn in_window(
    event: &CalendarEvent,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<bool, CalendarError> {
    if from.is_none() && to.is_none() {
        return Ok(true);
    }
    let from = from.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let to = to.unwrap_or(DateTime::<Utc>::MAX_UTC);
    match series_of(event)? {
        None => {
            let start = instant_of(&event.start_utc)?;
            Ok(start >= from && start < to)
        }
        Some(series) => {
            let mut steps = MAX_STEPS;
            Ok(!occurrences(&series, from, to, 1, &mut steps)
                .occurrences
                .is_empty())
        }
    }
}
