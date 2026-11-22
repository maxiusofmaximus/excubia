//! The events of a file, read into drafts the calendar can keep. An event it cannot keep is a
//! reason, not an error: the others still come in.

use chrono::NaiveDate;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::reader::{calendar, Component};
use super::text::{one_line, truncate_bytes, truncate_chars, unescape};
use super::time::{duration, stamp, trigger_minutes, Stamp};
use super::{IcsError, SkipReason, DETAILS_MAX, MAX_VEVENTS, UID_MAX_BYTES};
use crate::model_calendar::{
    NewEvent, EVENT_LOCATION_MAX_CHARS, EVENT_NOTES_MAX_BYTES, EVENT_TITLE_MAX_CHARS,
};
use crate::recurrence::{RecurrenceError, Rule};
use crate::timezone::EventTime;

/// One `VEVENT` of a file, by its place among them, and what became of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub index: usize,
    pub outcome: Result<Draft, SkipReason>,
}

/// An event the calendar can keep. `uid` is the file's, or one made from the event's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub uid: String,
    pub event: NewEvent,
}

/// An event that was left out, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub index: usize,
    pub reason: SkipReason,
}

/// What an import did: how many events came in, how many were left out and why. Only the
/// first `DETAILS_MAX` left-out events are named; the counts cover all of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped_existing: usize,
    pub skipped_unsupported: usize,
    pub details: Vec<Skipped>,
}

impl ImportReport {
    pub fn skip(&mut self, index: usize, reason: SkipReason) {
        match reason {
            SkipReason::Exists => self.skipped_existing += 1,
            _ => self.skipped_unsupported += 1,
        }
        if self.details.len() < DETAILS_MAX {
            self.details.push(Skipped { index, reason });
        }
    }
}

/// The events of a file, in file order. A file with more than `MAX_VEVENTS` is refused whole.
pub fn read_calendar(bytes: &[u8]) -> Result<Vec<Parsed>, IcsError> {
    let root = calendar(bytes)?;
    let events: Vec<&Component> = root
        .children
        .iter()
        .filter(|child| child.name == "VEVENT")
        .collect();
    if events.len() > MAX_VEVENTS {
        return Err(IcsError::TooManyEvents);
    }
    Ok(events
        .into_iter()
        .enumerate()
        .map(|(index, event)| Parsed {
            index,
            outcome: draft(event),
        })
        .collect())
}

fn draft(event: &Component) -> Result<Draft, SkipReason> {
    if event.prop("RECURRENCE-ID").is_some() {
        return Err(SkipReason::ExceptionInstance);
    }
    let (start, end, all_day) = timing_of(event)?;
    let title = summary_of(event)?;
    let uid = uid_of(event)?;
    let recurrence = recurrence_of(event)?;
    Ok(Draft {
        uid,
        event: NewEvent {
            title,
            start,
            end,
            all_day,
            location: location_of(event),
            notes: notes_of(event),
            reminder_minutes: reminder_of(event),
            recurrence,
        },
    })
}

/// The start, the end and whether the event is all day. An all-day end is the last day it
/// covers, so the file's exclusive end is one day earlier; an end on the start day is none.
fn timing_of(event: &Component) -> Result<(EventTime, Option<EventTime>, bool), SkipReason> {
    let start = stamp(event.prop("DTSTART").ok_or(SkipReason::NoStart)?)?;
    let end = event.prop("DTEND").map(stamp).transpose()?;
    let span = event
        .prop("DURATION")
        .map(|prop| duration(&prop.value).ok_or(SkipReason::InvalidTime))
        .transpose()?;
    let end = match (end, span) {
        (end, None) => end,
        (None, Some(span)) => Some(shifted(start, span).ok_or(SkipReason::InvalidTime)?),
        (Some(_), Some(_)) => return Err(SkipReason::InvalidTime),
    };
    match start {
        Stamp::Date(day) => all_day(day, end),
        _ => Ok((event_time(start), timed_end(start, end)?, false)),
    }
}

fn all_day(
    day: NaiveDate,
    until: Option<Stamp>,
) -> Result<(EventTime, Option<EventTime>, bool), SkipReason> {
    let end = match until {
        None => None,
        Some(Stamp::Date(until)) if until < day => return Err(SkipReason::InvalidTime),
        Some(Stamp::Date(until)) if until > day => Some(EventTime::Date(
            until.pred_opt().ok_or(SkipReason::InvalidTime)?,
        )),
        Some(Stamp::Date(_)) => None,
        Some(_) => return Err(SkipReason::InvalidTime),
    };
    Ok((EventTime::Date(day), end, true))
}

/// A timed end must be the same kind of clock as the start and not before it.
fn timed_end(start: Stamp, end: Option<Stamp>) -> Result<Option<EventTime>, SkipReason> {
    match end {
        None => Ok(None),
        Some(end) if !same_kind(start, end) || end < start => Err(SkipReason::InvalidTime),
        Some(end) => Ok(Some(event_time(end))),
    }
}

fn same_kind(start: Stamp, end: Stamp) -> bool {
    matches!(
        (start, end),
        (Stamp::Floating(_), Stamp::Floating(_)) | (Stamp::Utc(_), Stamp::Utc(_))
    )
}

fn shifted(start: Stamp, span: chrono::Duration) -> Option<Stamp> {
    match start {
        Stamp::Date(day) => day.checked_add_signed(span).map(Stamp::Date),
        Stamp::Floating(at) => at.checked_add_signed(span).map(Stamp::Floating),
        Stamp::Utc(at) => at.checked_add_signed(span).map(Stamp::Utc),
    }
}

fn event_time(stamp: Stamp) -> EventTime {
    match stamp {
        Stamp::Date(day) => EventTime::Date(day),
        Stamp::Floating(at) => EventTime::Local(at),
        Stamp::Utc(at) => EventTime::Offset(at.fixed_offset()),
    }
}

fn summary_of(event: &Component) -> Result<String, SkipReason> {
    let text = event
        .prop("SUMMARY")
        .map(|prop| one_line(&unescape(&prop.value)))
        .unwrap_or_default();
    if text.is_empty() {
        return Err(SkipReason::InvalidText);
    }
    Ok(truncate_chars(&text, EVENT_TITLE_MAX_CHARS).to_string())
}

fn uid_of(event: &Component) -> Result<String, SkipReason> {
    match event
        .prop("UID")
        .map(|prop| prop.value.trim())
        .filter(|uid| !uid.is_empty())
    {
        Some(uid) if uid.len() > UID_MAX_BYTES => Err(SkipReason::InvalidText),
        Some(uid) => Ok(uid.to_string()),
        None => Ok(generated_uid(event)),
    }
}

/// A UID made from what the event says, so the same event read twice gets the same one.
fn generated_uid(event: &Component) -> String {
    let mut hasher = Sha256::new();
    for name in ["SUMMARY", "DTSTART", "DTEND", "DURATION", "RRULE"] {
        if let Some(prop) = event.prop(name) {
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            hasher.update(prop.value.as_bytes());
            hasher.update(b"\n");
        }
    }
    format!("imp-{}", hex::encode(&hasher.finalize()[..16]))
}

fn recurrence_of(event: &Component) -> Result<Option<Rule>, SkipReason> {
    if event.prop("EXDATE").is_some() || event.prop("RDATE").is_some() {
        return Err(SkipReason::UnsupportedRecurrence);
    }
    let Some(prop) = event.prop("RRULE") else {
        return Ok(None);
    };
    Rule::parse(&prop.value)
        .map(Some)
        .map_err(|error| match error {
            RecurrenceError::Unsupported(_) => SkipReason::UnsupportedRecurrence,
            RecurrenceError::Invalid(_) => SkipReason::InvalidRecurrence,
        })
}

fn location_of(event: &Component) -> Option<String> {
    event
        .prop("LOCATION")
        .map(|prop| one_line(&unescape(&prop.value)))
        .filter(|text| !text.is_empty())
        .map(|text| truncate_chars(&text, EVENT_LOCATION_MAX_CHARS).to_string())
}

fn notes_of(event: &Component) -> Option<String> {
    event
        .prop("DESCRIPTION")
        .map(|prop| unescape(&prop.value))
        .filter(|text| !text.trim().is_empty())
        .map(|text| truncate_bytes(&text, EVENT_NOTES_MAX_BYTES).to_string())
}

/// The first `VALARM` gives the reminder, when its trigger is a whole number of minutes before
/// the start. A trigger relative to the end is not kept.
fn reminder_of(event: &Component) -> Option<i64> {
    let alarm = event.children.iter().find(|child| child.name == "VALARM")?;
    let trigger = alarm.prop("TRIGGER")?;
    if trigger
        .param("RELATED")
        .is_some_and(|related| related != "START")
    {
        return None;
    }
    trigger_minutes(&trigger.value)
}
