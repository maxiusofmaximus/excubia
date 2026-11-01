//! The calendar's models (CALENDARIO-F4): what an event is, what a create or an edit
//! carries, and the limits every surface (tools, REST, store) shares.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::recurrence::Rule;
use crate::timezone::EventTime;

/// The events one company keeps.
pub const MAX_EVENTS: i64 = 5_000;
pub const EVENT_TITLE_MAX_CHARS: usize = 120;
pub const EVENT_LOCATION_MAX_CHARS: usize = 200;
pub const EVENT_NOTES_MAX_BYTES: usize = 4 * 1024;
/// A reminder fires this many minutes before the start: up to one week.
pub const REMINDER_MAX_MINUTES: i64 = 10_080;
/// The most events one list returns.
pub const EVENT_LIST_MAX: usize = 100;
/// A list window spans at most this many days.
pub const EVENT_WINDOW_MAX_DAYS: i64 = 366;
/// A reminder that was due more than this long ago is dropped, not shown late.
pub const REMINDER_CATCH_UP_SECS: i64 = 24 * 60 * 60;

/// The tool that asks the user to delete an event: its approval is the user's.
pub const CALENDAR_DELETE_TOOL: &str = "calendar.delete";

/// An event as the store keeps it and the API shows it. Instants are UTC text, the same
/// `%Y-%m-%dT%H:%M:%S%.3fZ` the rest of the chat keeps, so they sort as strings;
/// `timezone` is the zone the event was written in, copied from the company at creation.
/// A recurring event keeps its rule as text in `recurrence`; `start_utc` is its first occurrence.
#[derive(Clone, Debug, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start_utc: String,
    pub end_utc: Option<String>,
    pub all_day: bool,
    pub recurrence: Option<String>,
    pub timezone: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub reminder_minutes: Option<i64>,
    pub reminded_at: Option<String>,
    pub reminded_occurrence: Option<String>,
    /// The UID an imported event came with; a host event has none and exports as `<id>@excubia`.
    pub uid: Option<String>,
    pub origin_kind: String,
    pub origin_id: String,
    pub conversation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One entry of a list: an event, or one occurrence of a recurring event. `id` is the event's
/// id in both cases; `occurrence_start` tells the occurrences of one series apart.
#[derive(Clone, Debug, Serialize)]
pub struct Listed {
    #[serde(flatten)]
    pub event: CalendarEvent,
    pub occurrence_start: String,
    pub recurring: bool,
}

impl Listed {
    /// The `after` that resumes a list just past this entry.
    pub fn cursor(&self) -> String {
        match self.recurring {
            true => format!("{}@{}", self.event.id, self.occurrence_start),
            false => self.event.id.clone(),
        }
    }

    /// The order a list sorts by: the start, then the id.
    pub fn key(&self) -> (&str, &str) {
        (&self.occurrence_start, &self.event.id)
    }
}

/// A list page: the entries, and whether the page is full or the expansion ran out of
/// budget, so more may follow after the last entry.
#[derive(Clone, Debug, Serialize)]
pub struct Listing {
    pub events: Vec<Listed>,
    pub truncated: bool,
}

/// What a create carries. An all-day event takes dates; its end is the next day's start
/// when the caller names none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewEvent {
    pub title: String,
    pub start: EventTime,
    pub end: Option<EventTime>,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub reminder_minutes: Option<i64>,
    pub recurrence: Option<Rule>,
}

/// What an edit changes. A field left as `None` keeps its value; `Some(None)` clears an
/// optional one (an end, a reminder, a recurrence). An empty location or note clears it as well.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventPatch {
    pub title: Option<String>,
    pub start: Option<EventTime>,
    pub end: Option<Option<EventTime>>,
    pub all_day: Option<bool>,
    pub location: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub reminder_minutes: Option<Option<i64>>,
    pub recurrence: Option<Option<Rule>>,
}

/// The window a list covers: `from` inclusive, `to` exclusive, both UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}
