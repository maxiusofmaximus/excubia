//! The reminder cards in a conversation's timeline (CALENDARIO-F4). A card names its event
//! and keeps no title or time of its own: both are read from the event when the timeline
//! loads, and deleting the event takes the card with it.

use rusqlite::{params, Connection};

use crate::error::{CalendarError, ChatError};
use crate::model::{CalendarCardLine, TimelineEntry};
use crate::timezone::utc_to_local;

use super::calendar_time::{instant_of, zone_of};

/// The reminder cards of a conversation, newest first, as timeline entries.
pub(super) fn load_calendar_cards(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ChatError> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.seq, c.created_at, e.id, e.title, COALESCE(c.occurrence_start, e.start_utc),
                e.all_day, e.timezone
           FROM timeline_calendar_card c
           JOIN calendar_event e ON e.id = c.event_id
          WHERE c.conversation_id = ?1 AND c.seq < ?2
          ORDER BY c.seq DESC LIMIT ?3",
    )?;
    let before = before_seq.unwrap_or(i64::MAX);
    let rows = statement
        .query_map(params![conversation_id, before, limit as i64], |row| {
            Ok(CardRow {
                card_id: row.get(0)?,
                seq: row.get(1)?,
                created_at: row.get(2)?,
                event_id: row.get(3)?,
                title: row.get(4)?,
                start_utc: row.get(5)?,
                all_day: row.get::<_, i64>(6)? != 0,
                zone: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|row| {
            let line = row
                .into_line()
                .map_err(|_| ChatError::Inconsistent("calendar event"))?;
            Ok(TimelineEntry::calendar_reminder(line))
        })
        .collect()
}

/// One card row joined to its event, before the time is read in the event's zone.
struct CardRow {
    card_id: String,
    seq: i64,
    created_at: String,
    event_id: String,
    title: String,
    start_utc: String,
    all_day: bool,
    zone: String,
}

impl CardRow {
    fn into_line(self) -> Result<CalendarCardLine, CalendarError> {
        let summary = match self.all_day {
            true => "día completo".to_owned(),
            false => {
                let start = instant_of(&self.start_utc)?;
                let local = utc_to_local(start, zone_of(&self.zone)?);
                format!("a las {}", local.format("%H:%M"))
            }
        };
        Ok(CalendarCardLine {
            seq: self.seq,
            card_id: self.card_id,
            ref_id: self.event_id,
            title: format!("Recordatorio: {}", self.title),
            summary,
            created_at: self.created_at,
        })
    }
}
