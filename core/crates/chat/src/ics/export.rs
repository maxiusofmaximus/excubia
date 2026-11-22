//! The company's events as one `.ics` file. Every event is written once, a series with its rule;
//! times are UTC, and an all-day event is the dates it covers in the zone it was written in.

use chrono::{DateTime, Utc};

use super::text::{escape, push_folded};
use super::HOST_UID_SUFFIX;
use crate::error::ChatError;
use crate::model_calendar::CalendarEvent;
use crate::timezone::{parse_zone, utc_to_local};

/// The file holding `events`, CRLF endings and every line folded at 75 octets.
pub fn export_calendar(events: &[CalendarEvent]) -> Result<String, ChatError> {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Excubia//Calendario//ES\r\nCALSCALE:GREGORIAN\r\n",
    );
    for event in events {
        vevent(&mut out, event)?;
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}

fn vevent(out: &mut String, event: &CalendarEvent) -> Result<(), ChatError> {
    let zone = parse_zone(&event.timezone).ok_or(ChatError::Inconsistent("event time zone"))?;
    let start = instant(&event.start_utc)?;
    let end = event.end_utc.as_deref().map(instant).transpose()?;
    let uid = match &event.uid {
        Some(uid) => uid.clone(),
        None => format!("{}{HOST_UID_SUFFIX}", event.id),
    };
    out.push_str("BEGIN:VEVENT\r\n");
    push_folded(out, &format!("UID:{uid}"));
    push_folded(
        out,
        &format!("DTSTAMP:{}", utc_stamp(instant(&event.updated_at)?)),
    );
    timing(out, event, zone, start, end);
    push_folded(out, &format!("SUMMARY:{}", escape(&event.title)));
    if let Some(location) = &event.location {
        push_folded(out, &format!("LOCATION:{}", escape(location)));
    }
    if let Some(notes) = &event.notes {
        push_folded(out, &format!("DESCRIPTION:{}", escape(notes)));
    }
    if let Some(rule) = &event.recurrence {
        push_folded(out, &format!("RRULE:{rule}"));
    }
    if let Some(minutes) = event.reminder_minutes {
        out.push_str("BEGIN:VALARM\r\n");
        push_folded(out, &format!("TRIGGER:-PT{minutes}M"));
        push_folded(out, "ACTION:DISPLAY");
        push_folded(out, &format!("DESCRIPTION:{}", escape(&event.title)));
        out.push_str("END:VALARM\r\n");
    }
    out.push_str("END:VEVENT\r\n");
    Ok(())
}

/// An all-day event is written as dates: its end is the day after its last, the way the file
/// means it. A timed event is written as UTC instants.
fn timing(
    out: &mut String,
    event: &CalendarEvent,
    zone: chrono_tz::Tz,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
) {
    if event.all_day {
        let day = |at: DateTime<Utc>| utc_to_local(at, zone).date().format("%Y%m%d").to_string();
        push_folded(out, &format!("DTSTART;VALUE=DATE:{}", day(start)));
        if let Some(end) = end {
            push_folded(out, &format!("DTEND;VALUE=DATE:{}", day(end)));
        }
    } else {
        push_folded(out, &format!("DTSTART:{}", utc_stamp(start)));
        if let Some(end) = end {
            push_folded(out, &format!("DTEND:{}", utc_stamp(end)));
        }
    }
}

fn utc_stamp(at: DateTime<Utc>) -> String {
    at.format("%Y%m%dT%H%M%SZ").to_string()
}

/// The instant an event keeps as text; a value that is not one is a store that broke its own rule.
fn instant(text: &str) -> Result<DateTime<Utc>, ChatError> {
    DateTime::parse_from_rfc3339(text)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|_| ChatError::Inconsistent("event instant"))
}
