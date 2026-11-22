//! Reminders (CALENDARIO-F4, CALENDARIO-F4b): an event with `reminder_minutes` is due at its start
//! minus that many minutes. A recurring event is due once per occurrence, and its watermark
//! `reminded_occurrence` holds the last occurrence it fired for. Delivery leaves one card in a
//! conversation and marks the reminder delivered in the same transaction, so a reminder arrives
//! once, even after a restart. A reminder more than 24 hours late is marked without a card.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;

use super::calendar_series::{due_occurrence, instant_at, recurring_events};
use super::calendar_time::instant_text;
use super::journal::append_event;
use super::lookup::next_seq;
use crate::clock;
use crate::error::CalendarError;
use crate::id;
use crate::model::ChatFrame;
use crate::model_calendar::{CalendarEvent, REMINDER_CATCH_UP_SECS};

/// The plain reminders due at `?1`, earliest first. `fire_at` is computed by SQLite from the
/// stored start, so the rule lives in one place and no derived column can go stale.
const DUE_SQL: &str = "SELECT id, conversation_id,
        strftime('%Y-%m-%dT%H:%M:%fZ', start_utc, '-' || reminder_minutes || ' minutes') AS fire_at
   FROM calendar_event
  WHERE reminder_minutes IS NOT NULL AND reminded_at IS NULL AND recurrence IS NULL
    AND strftime('%Y-%m-%dT%H:%M:%fZ', start_utc, '-' || reminder_minutes || ' minutes') <= ?1
  ORDER BY fire_at, id";

struct Due {
    id: String,
    conversation_id: Option<String>,
    fire_at: String,
}

impl super::ChatStore {
    /// Delivers the reminders that have come due: one card each, and every one of them is
    /// marked as delivered. A reminder older than the catch-up window, or with no open
    /// conversation to show it in, is marked without a card. Returns how many cards it left.
    pub fn deliver_due_reminders(&self) -> Result<usize, CalendarError> {
        let now_ms = self.now_ms();
        let now = clock::iso(now_ms);
        let cutoff = clock::iso(now_ms - REMINDER_CATCH_UP_SECS * 1000);
        let now_instant = instant_at(now_ms)?;
        let cutoff_instant = instant_at(now_ms - REMINDER_CATCH_UP_SECS * 1000)?;
        let mut frames: Vec<ChatFrame> = Vec::new();
        let cards = {
            let mut connection = self.lock();
            let transaction = connection.transaction()?;
            let mut cards = 0;
            for reminder in due_reminders(&transaction, &now)? {
                let conversation = if reminder.fire_at < cutoff {
                    None
                } else {
                    reminder_conversation(&transaction, reminder.conversation_id.as_deref())?
                };
                if let Some(conversation_id) = conversation {
                    frames.push(insert_card(
                        &transaction,
                        &conversation_id,
                        &reminder.id,
                        None,
                        &now,
                    )?);
                    cards += 1;
                }
                transaction.execute(
                    "UPDATE calendar_event SET reminded_at = ?2 WHERE id = ?1",
                    params![reminder.id, now],
                )?;
            }
            for event in recurring_events(&transaction)? {
                cards += deliver_series(
                    &transaction,
                    event,
                    now_instant,
                    cutoff_instant,
                    &mut frames,
                )?;
            }
            transaction.commit()?;
            cards
        };
        self.publish_all(frames);
        Ok(cards)
    }
}

/// How many reminders are due at `now_ms` and not yet delivered: a plain event counts once, and a
/// series counts once while one of its occurrences is due.
pub(super) fn due_count(connection: &Connection, now_ms: i64) -> Result<usize, CalendarError> {
    let mut statement = connection.prepare(DUE_SQL)?;
    let rows = statement.query_map([clock::iso(now_ms)], |row| row.get::<_, String>(0))?;
    let plain = rows.collect::<Result<Vec<_>, _>>()?.len();
    let now = instant_at(now_ms)?;
    let cutoff = instant_at(now_ms - REMINDER_CATCH_UP_SECS * 1000)?;
    let mut series = 0;
    for event in recurring_events(connection)? {
        if due_occurrence(&event, now, cutoff)?.is_some() {
            series += 1;
        }
    }
    Ok(plain + series)
}

/// Delivers the occurrences of one series that have come due, one card each, and moves the
/// watermark past every one of them. Returns how many cards it left.
fn deliver_series(
    transaction: &Transaction<'_>,
    mut event: CalendarEvent,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    frames: &mut Vec<ChatFrame>,
) -> Result<usize, CalendarError> {
    let mut cards = 0;
    while let Some(occurrence) = due_occurrence(&event, now, cutoff)? {
        let at = instant_text(occurrence);
        if let Some(conversation_id) =
            reminder_conversation(transaction, event.conversation_id.as_deref())?
        {
            frames.push(insert_card(
                transaction,
                &conversation_id,
                &event.id,
                Some(&at),
                &at,
            )?);
            cards += 1;
        }
        transaction.execute(
            "UPDATE calendar_event SET reminded_occurrence = ?2 WHERE id = ?1",
            params![event.id, at],
        )?;
        event.reminded_occurrence = Some(at);
    }
    Ok(cards)
}

fn due_reminders(transaction: &Transaction<'_>, now: &str) -> Result<Vec<Due>, rusqlite::Error> {
    let mut statement = transaction.prepare(DUE_SQL)?;
    let rows = statement.query_map([now], |row| {
        Ok(Due {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            fire_at: row.get(2)?,
        })
    })?;
    rows.collect()
}

/// The conversation a reminder shows in: the one its event was made in while it is open,
/// else the most recently active open conversation of the company.
fn reminder_conversation(
    transaction: &Transaction<'_>,
    origin: Option<&str>,
) -> Result<Option<String>, rusqlite::Error> {
    if let Some(origin) = origin {
        let open: Option<String> = transaction
            .query_row(
                "SELECT id FROM conversation WHERE id = ?1 AND archived = 0",
                [origin],
                |row| row.get(0),
            )
            .optional()?;
        if open.is_some() {
            return Ok(open);
        }
    }
    transaction
        .query_row(
            "SELECT id FROM conversation WHERE archived = 0
             ORDER BY last_message_at DESC, id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
}

/// Leaves the card in the conversation's timeline and announces it, as the other cards do.
/// `occurrence` names the occurrence of a series; a plain event's card keeps none.
fn insert_card(
    transaction: &Transaction<'_>,
    conversation_id: &str,
    event_id: &str,
    occurrence: Option<&str>,
    now: &str,
) -> Result<ChatFrame, CalendarError> {
    let seq = next_seq(transaction, conversation_id)?;
    let card_id = id::new_id("cc");
    transaction.execute(
        "INSERT INTO timeline_calendar_card (id, conversation_id, event_id, seq, created_at,
             occurrence_start)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![card_id, conversation_id, event_id, seq, now, occurrence],
    )?;
    Ok(append_event(
        transaction,
        "chat.card",
        conversation_id,
        json!({
            "event": "chat.card",
            "conversation_id": conversation_id,
            "card_id": card_id,
            "kind": "calendar_reminder",
            "status": "reminder",
        }),
    )?)
}
