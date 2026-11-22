//! The calendar's wire shapes: the events the app keeps on the host and the zone the
//! company keeps them in. Field names follow `docs/contracts/api.md`, «Calendario de la app».

use serde::{Deserialize, Serialize};

/// One event. The instants are UTC; the screen shows and edits them in the company's zone.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start_utc: String,
    #[serde(default)]
    pub end_utc: Option<String>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Minutes before the start that the reminder fires; `None` has no reminder.
    #[serde(default)]
    pub reminder_minutes: Option<i64>,
    /// The RFC 5545 rule of a series; `None` for an event that happens once.
    #[serde(default)]
    pub recurrence: Option<String>,
    /// The instant this row stands for. A list row of a series is one occurrence; the
    /// event itself, as the host saves it, has none.
    #[serde(default)]
    pub occurrence_start: Option<String>,
    /// True on a list row that is one occurrence of a series.
    #[serde(default)]
    pub recurring: bool,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EventPage {
    #[serde(default)]
    pub events: Vec<CalendarEvent>,
    /// The host stopped early: the page is full, or its calculation ran out.
    #[serde(default)]
    pub truncated: bool,
    /// The `after` of the next page; `None` unless the page is truncated.
    #[serde(default)]
    pub next: Option<String>,
}

/// The company's zone. `None` until someone fixes it, once.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompanyTimezone {
    #[serde(default)]
    pub timezone: Option<String>,
}

/// What the screen sends for an event. `start` and `end` are local to the company's
/// zone: `YYYY-MM-DDTHH:MM`, or `YYYY-MM-DD` for a whole day. `end: None` has no end;
/// `reminder_minutes: None` has no reminder; `recurrence: None` happens once. Every
/// field goes on each write.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventDraft {
    pub title: String,
    pub start: String,
    pub end: Option<String>,
    pub all_day: bool,
    pub location: String,
    pub notes: String,
    pub reminder_minutes: Option<i64>,
    pub recurrence: Option<String>,
}
