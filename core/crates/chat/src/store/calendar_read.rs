//! The calendar's reads (CALENDARIO-F4): a window of entries in start order, and the small
//! summary `/v1/state` carries (today's count and the next entry). A recurring event adds its
//! occurrences to both (CALENDARIO-F4b). Both read the store only; no zone means no day to count.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension};

use super::calendar::{company_zone_name, event_by_id, event_row, EVENT_SELECT};
use super::calendar_remind::due_count;
use super::calendar_series::{instant_at, recurring_events, series_of};
use super::calendar_time::{instant_of, instant_text, zone_of};
use crate::error::CalendarError;
use crate::model_calendar::{
    CalendarEvent, Listed, Listing, Window, EVENT_LIST_MAX, EVENT_WINDOW_MAX_DAYS,
};
use crate::recurrence_expand::{occurrences, Occurrence, MAX_STEPS};
use crate::timezone::{local_day_bounds, local_to_utc, utc_to_local, EventTime};

/// The calendar as `/v1/state` shows it: what is left of today, the next entry, and the
/// reminders due but not yet delivered. Event titles stay out of the state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSummary {
    pub today: i64,
    pub next: Option<NextEvent>,
    pub reminders_due: i64,
}

/// The next entry: an event, or the next occurrence of a series (its start is that occurrence).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextEvent {
    pub id: String,
    pub start_utc: String,
}

impl super::ChatStore {
    /// The entries that start inside the window, earliest first, at most `limit` (1 to 100).
    /// `after` names the last entry of the previous page: an event id, or `id@occurrence_start`.
    pub fn list_events(
        &self,
        window: Window,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Listing, CalendarError> {
        check_window(window)?;
        if !(1..=EVENT_LIST_MAX).contains(&limit) {
            return Err(CalendarError::InvalidField {
                field: "limit",
                message: "a list returns 1 to 100 events",
            });
        }
        let connection = self.lock();
        let key = cursor(&connection, after)?;
        let mut listed = plain_page(&connection, window, key.as_ref(), limit)?;
        let mut steps = MAX_STEPS;
        let mut exhausted = false;
        for event in recurring_events(&connection)? {
            let Some(series) = series_of(&event)? else {
                continue;
            };
            let from = match &key {
                Some((start, _)) => window.from.max(instant_of(start)?),
                None => window.from,
            };
            let found = occurrences(&series, from, window.to, limit + 1, &mut steps);
            exhausted |= found.exhausted;
            listed.extend(
                found
                    .occurrences
                    .into_iter()
                    .map(|occurrence| occurrence_entry(&event, occurrence))
                    .filter(|entry| after_key(entry, key.as_ref())),
            );
        }
        listed.sort_by(|a, b| a.key().cmp(&b.key()));
        listed.truncate(limit);
        let truncated = listed.len() == limit || exhausted;
        Ok(Listing {
            events: listed,
            truncated,
        })
    }

    /// The window two times name in the company's zone, `to` not included. A date names the
    /// start of its local day, so two dates cover whole days.
    pub fn calendar_window(&self, from: EventTime, to: EventTime) -> Result<Window, CalendarError> {
        let connection = self.lock();
        let zone_name = company_zone_name(&connection)?.ok_or(CalendarError::TimezoneUnset)?;
        let zone = zone_of(&zone_name)?;
        Ok(Window {
            from: window_instant(from, zone)?,
            to: window_instant(to, zone)?,
        })
    }

    /// The summary `/v1/state` carries, read at the store's clock.
    pub fn calendar_summary(&self) -> Result<CalendarSummary, CalendarError> {
        let now_ms = self.now_ms();
        let now = instant_at(now_ms)?;
        let connection = self.lock();
        let reminders_due = due_count(&connection, now_ms)? as i64;
        let next = next_entry(&connection, now)?;
        let Some(zone_name) = company_zone_name(&connection)? else {
            return Ok(CalendarSummary {
                today: 0,
                next,
                reminders_due,
            });
        };
        let zone = zone_of(&zone_name)?;
        let (from, to) = local_day_bounds(utc_to_local(now, zone).date(), zone)?;
        let today = count_between(&connection, from, to)?;
        Ok(CalendarSummary {
            today,
            next,
            reminders_due,
        })
    }
}

/// The events of the plain kind that start in the window, after the page's key, in order.
fn plain_page(
    connection: &Connection,
    window: Window,
    key: Option<&(String, String)>,
    limit: usize,
) -> Result<Vec<Listed>, CalendarError> {
    let (after_start, after_id) = key.map_or(("", ""), |(start, id)| (start.as_str(), id.as_str()));
    let mut statement = connection.prepare(&format!(
        "{EVENT_SELECT} WHERE recurrence IS NULL AND start_utc >= ?1 AND start_utc < ?2
           AND (start_utc, id) > (?3, ?4)
         ORDER BY start_utc, id LIMIT ?5"
    ))?;
    let rows = statement.query_map(
        params![
            instant_text(window.from),
            instant_text(window.to),
            after_start,
            after_id,
            limit as i64
        ],
        event_row,
    )?;
    let events: Vec<CalendarEvent> = rows.collect::<Result<_, _>>()?;
    Ok(events
        .into_iter()
        .map(|event| Listed {
            occurrence_start: event.start_utc.clone(),
            recurring: false,
            event,
        })
        .collect())
}

/// One occurrence of a series as an entry: the series' id, with the occurrence's own times.
fn occurrence_entry(event: &CalendarEvent, occurrence: Occurrence) -> Listed {
    let start = instant_text(occurrence.start);
    Listed {
        event: CalendarEvent {
            start_utc: start.clone(),
            end_utc: occurrence.end.map(instant_text),
            ..event.clone()
        },
        occurrence_start: start,
        recurring: true,
    }
}

fn after_key(entry: &Listed, key: Option<&(String, String)>) -> bool {
    match key {
        None => true,
        Some((start, id)) => entry.key() > (start.as_str(), id.as_str()),
    }
}

/// The first entry that starts at or after `now`: an event, or the next occurrence of a series.
fn next_entry(
    connection: &Connection,
    now: DateTime<Utc>,
) -> Result<Option<NextEvent>, CalendarError> {
    let mut best = connection
        .query_row(
            "SELECT id, start_utc FROM calendar_event WHERE recurrence IS NULL AND start_utc >= ?1
             ORDER BY start_utc, id LIMIT 1",
            [instant_text(now)],
            |row| {
                Ok(NextEvent {
                    id: row.get(0)?,
                    start_utc: row.get(1)?,
                })
            },
        )
        .optional()?;
    for event in recurring_events(connection)? {
        let Some(series) = series_of(&event)? else {
            continue;
        };
        let mut steps = MAX_STEPS;
        let found = occurrences(&series, now, DateTime::<Utc>::MAX_UTC, 1, &mut steps);
        let Some(occurrence) = found.occurrences.first() else {
            continue;
        };
        let start = instant_text(occurrence.start);
        match &best {
            Some(current) if current.start_utc <= start => {}
            _ => {
                best = Some(NextEvent {
                    id: event.id,
                    start_utc: start,
                })
            }
        }
    }
    Ok(best)
}

/// How many entries start from `from` up to `to` (not included), occurrences included.
fn count_between(
    connection: &Connection,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<i64, CalendarError> {
    let plain: i64 = connection.query_row(
        "SELECT COUNT(*) FROM calendar_event WHERE recurrence IS NULL
           AND start_utc >= ?1 AND start_utc < ?2",
        params![instant_text(from), instant_text(to)],
        |row| row.get(0),
    )?;
    let mut occurring = 0;
    for event in recurring_events(connection)? {
        let Some(series) = series_of(&event)? else {
            continue;
        };
        let mut steps = MAX_STEPS;
        occurring += occurrences(&series, from, to, usize::MAX, &mut steps)
            .occurrences
            .len() as i64;
    }
    Ok(plain + occurring)
}

pub(super) fn window_instant(time: EventTime, zone: Tz) -> Result<DateTime<Utc>, CalendarError> {
    match time {
        EventTime::Date(day) => Ok(local_day_bounds(day, zone)?.0),
        EventTime::Local(local) => local_to_utc(local, zone),
        EventTime::Offset(instant) => Ok(instant.with_timezone(&Utc)),
    }
}

fn check_window(window: Window) -> Result<(), CalendarError> {
    if window.to <= window.from {
        return Err(CalendarError::InvalidField {
            field: "to",
            message: "the window ends after it starts",
        });
    }
    if window.to - window.from > Duration::days(EVENT_WINDOW_MAX_DAYS) {
        return Err(CalendarError::InvalidField {
            field: "to",
            message: "the window spans at most 366 days",
        });
    }
    Ok(())
}

/// The sort key of the entry a page follows, `None` for the first page. A bare id names an
/// event by its start; `id@occurrence_start` names one occurrence of a series.
fn cursor(
    connection: &Connection,
    after: Option<&str>,
) -> Result<Option<(String, String)>, CalendarError> {
    let Some(after) = after else {
        return Ok(None);
    };
    match after.split_once('@') {
        Some((id, start)) => {
            let start = instant_of(start)
                .map_err(|_| bad_cursor("an event id, or an id and its occurrence start"))?;
            Ok(Some((instant_text(start), id.to_string())))
        }
        None => {
            let event = event_by_id(connection, after).map_err(|error| match error {
                CalendarError::EventNotFound(_) => bad_cursor("no such event"),
                other => other,
            })?;
            Ok(Some((event.start_utc, event.id)))
        }
    }
}

fn bad_cursor(message: &'static str) -> CalendarError {
    CalendarError::InvalidField {
        field: "after",
        message,
    }
}
