//! The calendar's writes (CALENDARIO-F4): an event created, edited or deleted, and the
//! company's zone. A write and its `agenda_audit` line share one transaction, and the
//! audit keeps metadata only. Alfred's delete is never made here on its own word: the
//! user's approval burns it through [`delete_in`]; the app's delete runs at once.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::agenda::{write_audit, Writer};
use super::calendar_series::check_series;
use super::calendar_time::{current_times, instant_text, resolve, zone_of};
use crate::error::{CalendarError, ChatError};
use crate::id;
use crate::model_calendar::{CalendarEvent, EventPatch, NewEvent, MAX_EVENTS};
use crate::seat::SeatActor;
use crate::timezone::parse_zone;

/// Who writes an event: Alfred for the user in a conversation, or the app for the user.
#[derive(Clone, Copy, Debug)]
pub enum EventOrigin<'a> {
    Seat(&'a SeatActor),
    Device(&'a str),
}

pub(super) const EVENT_SELECT: &str = "SELECT id, title, start_utc, end_utc, all_day, timezone,
    location, notes, reminder_minutes, reminded_at, origin_kind, origin_id, conversation_id,
    created_at, updated_at, recurrence, reminded_occurrence, uid FROM calendar_event";

impl super::ChatStore {
    /// Create an event in the company's zone. Without a zone nothing is written.
    pub fn create_event(
        &self,
        origin: EventOrigin<'_>,
        new: NewEvent,
    ) -> Result<CalendarEvent, CalendarError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let zone_name = company_zone_name(&transaction)?.ok_or(CalendarError::TimezoneUnset)?;
        let (writer, conversation_id) = match origin {
            EventOrigin::Seat(actor) => (
                Writer::seat(actor),
                Some(actor.conversation_id.clone()).filter(|id| !id.is_empty()),
            ),
            EventOrigin::Device(device_id) => (Writer::device(device_id), None),
        };
        let event = new_event(&zone_name, new, &writer, conversation_id, None, &now)?;
        check_series(&event)?;
        insert_event(&transaction, writer, &event, &now)?;
        transaction.commit()?;
        Ok(event)
    }

    /// Change the fields the patch names. Times are read in the event's own zone, the one
    /// it was written in. A moved start or a changed reminder arms the reminder again.
    pub fn update_event(
        &self,
        origin: EventOrigin<'_>,
        event_id: &str,
        patch: EventPatch,
    ) -> Result<CalendarEvent, CalendarError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let mut event = event_by_id(&transaction, event_id)?;
        let writer = match origin {
            EventOrigin::Seat(actor) => Writer::seat(actor),
            EventOrigin::Device(device_id) => Writer::device(device_id),
        };
        apply(&mut event, patch)?;
        check_series(&event)?;
        event.updated_at = now.clone();
        transaction.execute(
            "UPDATE calendar_event SET title = ?2, start_utc = ?3, end_utc = ?4, all_day = ?5,
                 location = ?6, notes = ?7, reminder_minutes = ?8, reminded_at = ?9,
                 updated_at = ?10, updated_by = ?11, recurrence = ?12, reminded_occurrence = ?13
              WHERE id = ?1",
            params![
                event.id,
                event.title,
                event.start_utc,
                event.end_utc,
                event.all_day,
                event.location,
                event.notes,
                event.reminder_minutes,
                event.reminded_at,
                now,
                writer.kind,
                event.recurrence,
                event.reminded_occurrence
            ],
        )?;
        write_audit(
            &transaction,
            "calendar_event",
            &event.id,
            "update",
            writer.kind,
            writer.id,
            &now,
        )?;
        transaction.commit()?;
        Ok(event)
    }

    /// Delete an event the app asks to delete. The user asked for it, so it runs at once.
    pub fn delete_event_by_device(
        &self,
        device_id: &str,
        event_id: &str,
    ) -> Result<(), CalendarError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        event_by_id(&transaction, event_id)?;
        delete_in(&transaction, event_id, Writer::device(device_id), &now)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn event(&self, event_id: &str) -> Result<CalendarEvent, CalendarError> {
        let connection = self.lock();
        event_by_id(&connection, event_id)
    }

    /// The zone the company keeps its calendar in, `None` until someone sets it.
    pub fn company_timezone(&self) -> Result<Option<String>, CalendarError> {
        let connection = self.lock();
        Ok(company_zone_name(&connection)?)
    }

    /// Fix the company's zone, once. Only an IANA name is kept; the app is the one that sets it.
    pub fn set_company_timezone(&self, device_id: &str, name: &str) -> Result<(), CalendarError> {
        if parse_zone(name).is_none() {
            return Err(CalendarError::InvalidZone);
        }
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let company_id: String =
            transaction.query_row("SELECT id FROM company LIMIT 1", [], |row| row.get(0))?;
        if company_zone_name(&transaction)?.is_some() {
            return Err(CalendarError::TimezoneAlreadySet);
        }
        transaction.execute(
            "UPDATE company SET timezone = ?1 WHERE id = ?2",
            params![name, company_id],
        )?;
        write_audit(
            &transaction,
            "company",
            &company_id,
            "timezone",
            "device",
            device_id,
            &now,
        )?;
        transaction.commit()?;
        Ok(())
    }
}

/// Deletes an event inside the transaction that burns the user's approval, or that the
/// app's delete opens. An event already gone is no failure here.
pub(super) fn delete_in(
    transaction: &Transaction<'_>,
    event_id: &str,
    writer: Writer<'_>,
    at: &str,
) -> Result<(), ChatError> {
    if transaction.execute("DELETE FROM calendar_event WHERE id = ?1", [event_id])? == 1 {
        write_audit(
            transaction,
            "calendar_event",
            event_id,
            "delete",
            writer.kind,
            writer.id,
            at,
        )?;
    }
    Ok(())
}

pub(super) fn event_by_id(
    connection: &Connection,
    event_id: &str,
) -> Result<CalendarEvent, CalendarError> {
    connection
        .query_row(
            &format!("{EVENT_SELECT} WHERE id = ?1"),
            [event_id],
            event_row,
        )
        .optional()?
        .ok_or_else(|| CalendarError::EventNotFound(event_id.to_string()))
}

pub(super) fn event_row(row: &Row<'_>) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id: row.get(0)?,
        title: row.get(1)?,
        start_utc: row.get(2)?,
        end_utc: row.get(3)?,
        all_day: row.get::<_, i64>(4)? != 0,
        recurrence: row.get(15)?,
        timezone: row.get(5)?,
        location: row.get(6)?,
        notes: row.get(7)?,
        reminder_minutes: row.get(8)?,
        reminded_at: row.get(9)?,
        reminded_occurrence: row.get(16)?,
        uid: row.get(17)?,
        origin_kind: row.get(10)?,
        origin_id: row.get(11)?,
        conversation_id: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

/// The event a create carries, with its times resolved in the company's zone. The caller
/// checks the series and inserts it.
pub(super) fn new_event(
    zone_name: &str,
    new: NewEvent,
    writer: &Writer<'_>,
    conversation_id: Option<String>,
    uid: Option<String>,
    now: &str,
) -> Result<CalendarEvent, CalendarError> {
    let times = resolve(zone_of(zone_name)?, new.all_day, new.start, new.end)?;
    Ok(CalendarEvent {
        id: id::new_id("ev"),
        title: new.title,
        start_utc: instant_text(times.start),
        end_utc: times.end.map(instant_text),
        all_day: new.all_day,
        recurrence: new.recurrence.map(|rule| rule.to_string()),
        timezone: zone_name.to_string(),
        location: new.location.filter(|text| !text.is_empty()),
        notes: new.notes.filter(|text| !text.is_empty()),
        reminder_minutes: new.reminder_minutes,
        reminded_at: None,
        reminded_occurrence: None,
        uid,
        origin_kind: writer.kind.to_string(),
        origin_id: writer.id.to_string(),
        conversation_id,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

/// The company's zone name, `None` while it is unset.
pub(super) fn company_zone_name(
    connection: &Connection,
) -> Result<Option<String>, rusqlite::Error> {
    Ok(connection
        .query_row("SELECT timezone FROM company LIMIT 1", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten())
}

/// Inserts a new event and its audit line. The count and the insert share one transaction,
/// so two creates cannot both pass the limit.
pub(super) fn insert_event(
    transaction: &Transaction<'_>,
    writer: Writer<'_>,
    event: &CalendarEvent,
    now: &str,
) -> Result<(), CalendarError> {
    let count: i64 =
        transaction.query_row("SELECT COUNT(*) FROM calendar_event", [], |row| row.get(0))?;
    if count >= MAX_EVENTS {
        return Err(CalendarError::EventLimit);
    }
    transaction.execute(
        "INSERT INTO calendar_event (id, title, start_utc, end_utc, all_day, timezone, location,
             notes, reminder_minutes, reminded_at, reminded_occurrence, origin_kind, origin_id,
             conversation_id, created_at, updated_at, updated_by, recurrence, uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?12,
             ?16, ?17)",
        params![
            event.id,
            event.title,
            event.start_utc,
            event.end_utc,
            event.all_day,
            event.timezone,
            event.location,
            event.notes,
            event.reminder_minutes,
            event.reminded_at,
            event.reminded_occurrence,
            writer.kind,
            writer.id,
            event.conversation_id,
            now,
            event.recurrence,
            event.uid
        ],
    )?;
    write_audit(
        transaction,
        "calendar_event",
        &event.id,
        "create",
        writer.kind,
        writer.id,
        now,
    )?;
    Ok(())
}

/// Applies a patch to an event. The times are read back in the event's zone, so a patch
/// that touches one time or the all-day flag resolves the whole span again.
fn apply(event: &mut CalendarEvent, patch: EventPatch) -> Result<(), CalendarError> {
    let zone = zone_of(&event.timezone)?;
    let touches_time = patch.start.is_some() || patch.end.is_some() || patch.all_day.is_some();
    let rearms = touches_time || patch.reminder_minutes.is_some() || patch.recurrence.is_some();
    if let Some(title) = patch.title {
        event.title = title;
    }
    if let Some(recurrence) = patch.recurrence {
        event.recurrence = recurrence.map(|rule| rule.to_string());
    }
    if let Some(location) = patch.location {
        event.location = location.filter(|text| !text.is_empty());
    }
    if let Some(notes) = patch.notes {
        event.notes = notes.filter(|text| !text.is_empty());
    }
    if let Some(reminder) = patch.reminder_minutes {
        event.reminder_minutes = reminder;
    }
    if touches_time {
        let (current_start, current_end) = current_times(event, zone)?;
        let all_day = patch.all_day.unwrap_or(event.all_day);
        let start = patch.start.unwrap_or(current_start);
        let end = patch.end.unwrap_or(current_end);
        let times = resolve(zone, all_day, start, end)?;
        event.start_utc = instant_text(times.start);
        event.end_utc = times.end.map(instant_text);
        event.all_day = all_day;
    }
    if rearms {
        event.reminded_at = None;
        event.reminded_occurrence = None;
    }
    Ok(())
}
