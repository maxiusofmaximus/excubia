//! The `calendar.*` tools (CALENDARIO-F4): Alfred reads and keeps the user's calendar. The
//! parser bounds every field and reads each time the way the store does; the host decides
//! who may call a tool, reads the times in the company's zone and opens the question for
//! a delete.

use hn_core_chat::{
    EventPatch, EventTime, NewEvent, RecurrenceError, Rule, ToolError, ToolRequest, EVENT_LIST_MAX,
    EVENT_LOCATION_MAX_CHARS, EVENT_NOTES_MAX_BYTES, EVENT_TITLE_MAX_CHARS, REMINDER_MAX_MINUTES,
};
use serde::Deserialize;

use super::args::IdArgs;
use super::parse::{arguments, name_field, no_arguments, one_line};

/// The events a list returns when the caller names no limit.
const DEFAULT_LIST_LIMIT: usize = 20;

#[derive(Clone, Debug, PartialEq)]
pub enum CalendarCall {
    Timezone,
    List(ListCall),
    Create(NewEvent),
    /// A field that is `None` keeps its value; see [`update`] for how an empty one clears.
    Update {
        id: String,
        patch: EventPatch,
    },
    Delete {
        id: String,
    },
}

/// The events that start from `from` up to `to`, which is not included.
#[derive(Clone, Debug, PartialEq)]
pub struct ListCall {
    pub from: EventTime,
    pub to: EventTime,
    pub limit: usize,
    pub after: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    from: String,
    to: String,
    limit: Option<usize>,
    after: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    title: String,
    start: String,
    end: Option<String>,
    all_day: Option<bool>,
    location: Option<String>,
    notes: Option<String>,
    reminder_minutes: Option<i64>,
    recurrence: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    id: String,
    title: Option<String>,
    start: Option<String>,
    end: Option<String>,
    all_day: Option<bool>,
    location: Option<String>,
    notes: Option<String>,
    reminder_minutes: Option<i64>,
    recurrence: Option<String>,
}

pub(super) fn timezone(request: &ToolRequest) -> Result<CalendarCall, ToolError> {
    no_arguments(request)?;
    Ok(CalendarCall::Timezone)
}

pub(super) fn list(request: &ToolRequest) -> Result<CalendarCall, ToolError> {
    let args: ListArgs = arguments(request, "from")?;
    let limit = args.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    if !(1..=EVENT_LIST_MAX).contains(&limit) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "limit",
            format!("limit must be 1 to {EVENT_LIST_MAX}"),
        ));
    }
    Ok(CalendarCall::List(ListCall {
        from: time("from", &args.from)?,
        to: time("to", &args.to)?,
        limit,
        after: args.after.map(|id| name_field("after", &id)).transpose()?,
    }))
}

pub(super) fn create(request: &ToolRequest) -> Result<CalendarCall, ToolError> {
    let args: CreateArgs = arguments(request, "title")?;
    Ok(CalendarCall::Create(NewEvent {
        title: title(&args.title)?,
        start: time("start", &args.start)?,
        end: args
            .end
            .as_deref()
            .map(|raw| time("end", raw))
            .transpose()?,
        all_day: args.all_day.unwrap_or(false),
        location: args.location.as_deref().map(location).transpose()?,
        notes: args.notes.as_deref().map(notes).transpose()?,
        reminder_minutes: reminder(args.reminder_minutes)?,
        recurrence: args
            .recurrence
            .as_deref()
            .filter(|raw| !raw.trim().is_empty())
            .map(recurrence)
            .transpose()?,
    }))
}

/// An empty `end` removes the end, `reminder_minutes: -1` removes the reminder, and an empty
/// `recurrence` makes the event a single one again.
pub(super) fn update(request: &ToolRequest) -> Result<CalendarCall, ToolError> {
    let args: UpdateArgs = arguments(request, "id")?;
    let patch = EventPatch {
        title: args.title.as_deref().map(title).transpose()?,
        start: args
            .start
            .as_deref()
            .map(|raw| time("start", raw))
            .transpose()?,
        end: args
            .end
            .as_deref()
            .map(|raw| match raw.is_empty() {
                true => Ok(None),
                false => time("end", raw).map(Some),
            })
            .transpose()?,
        all_day: args.all_day,
        location: args
            .location
            .as_deref()
            .map(location)
            .transpose()?
            .map(Some),
        notes: args.notes.as_deref().map(notes).transpose()?.map(Some),
        reminder_minutes: match args.reminder_minutes {
            Some(-1) => Some(None),
            other => reminder(other)?.map(Some),
        },
        recurrence: match args.recurrence.as_deref() {
            None => None,
            Some(raw) if raw.trim().is_empty() => Some(None),
            Some(raw) => Some(Some(recurrence(raw)?)),
        },
    };
    if patch == EventPatch::default() {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "id",
            "update needs a field to change",
        ));
    }
    Ok(CalendarCall::Update {
        id: name_field("id", &args.id)?,
        patch,
    })
}

pub(super) fn delete(request: &ToolRequest) -> Result<CalendarCall, ToolError> {
    let args: IdArgs = arguments(request, "id")?;
    Ok(CalendarCall::Delete {
        id: name_field("id", &args.id)?,
    })
}

/// A date, a local time or a time with an offset, as the store reads them.
fn time(field: &'static str, raw: &str) -> Result<EventTime, ToolError> {
    EventTime::parse(raw.trim()).ok_or_else(|| {
        ToolError::invalid(
            "invalid_arguments",
            field,
            "a time is a date (2026-11-03), a local time (2026-11-03T09:30) or a time with an offset (2026-11-03T09:30+01:00)",
        )
    })
}

fn title(raw: &str) -> Result<String, ToolError> {
    let value = one_line("title", raw, EVENT_TITLE_MAX_CHARS)?;
    if value.is_empty() {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "title",
            "an event needs a title",
        ));
    }
    Ok(value)
}

fn location(raw: &str) -> Result<String, ToolError> {
    one_line("location", raw, EVENT_LOCATION_MAX_CHARS)
}

/// The notes, trimmed; they are bounded in bytes, so a long text is refused, never cut.
fn notes(raw: &str) -> Result<String, ToolError> {
    if raw.len() > EVENT_NOTES_MAX_BYTES {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "notes",
            format!("notes are at most {EVENT_NOTES_MAX_BYTES} bytes"),
        ));
    }
    Ok(raw.trim().to_string())
}

/// A repeat rule, read as the store reads it. An unsupported rule and a broken one get
/// different codes, so Alfred can tell the user which of the two it was.
fn recurrence(raw: &str) -> Result<Rule, ToolError> {
    Rule::parse(raw.trim()).map_err(|error| match error {
        RecurrenceError::Unsupported(message) => {
            ToolError::invalid("unsupported_recurrence", "recurrence", message)
        }
        RecurrenceError::Invalid(message) => {
            ToolError::invalid("invalid_recurrence", "recurrence", message)
        }
    })
}

fn reminder(minutes: Option<i64>) -> Result<Option<i64>, ToolError> {
    match minutes {
        Some(value) if !(0..=REMINDER_MAX_MINUTES).contains(&value) => Err(ToolError::invalid(
            "invalid_arguments",
            "reminder_minutes",
            format!("a reminder is 0 to {REMINDER_MAX_MINUTES} minutes before the start"),
        )),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    #[test]
    fn an_event_keeps_its_local_start_and_optional_fields() {
        let created = create(&request(
            "calendar.create",
            json!({ "title": "  Llamada con Ana  ", "start": "2026-11-03T09:30", "reminder_minutes": 15 }),
        ))
        .expect("event");
        let CalendarCall::Create(new) = created else {
            panic!("a create call");
        };
        assert_eq!(new.title, "Llamada con Ana");
        assert_eq!(new.reminder_minutes, Some(15));
        assert!(new.end.is_none() && !new.all_day);
    }

    #[test]
    fn an_event_refuses_a_missing_title_and_a_reminder_out_of_range() {
        assert!(create(&request(
            "calendar.create",
            json!({ "title": " ", "start": "2026-11-03" })
        ))
        .is_err());
        let late = json!({ "title": "x", "start": "2026-11-03T09:30", "reminder_minutes": REMINDER_MAX_MINUTES + 1 });
        assert!(create(&request("calendar.create", late)).is_err());
    }

    #[test]
    fn a_list_needs_a_limit_between_one_and_the_maximum() {
        let window = json!({ "from": "2026-11-01", "to": "2026-11-08" });
        assert!(list(&request("calendar.list", window.clone())).is_ok());
        let zero = json!({ "from": "2026-11-01", "to": "2026-11-08", "limit": 0 });
        assert!(list(&request("calendar.list", zero)).is_err());
    }

    #[test]
    fn an_empty_end_clears_it_and_minus_one_clears_the_reminder() {
        let cleared = update(&request(
            "calendar.update",
            json!({ "id": "ev_1", "end": "", "reminder_minutes": -1 }),
        ))
        .expect("update");
        let CalendarCall::Update { id, patch } = cleared else {
            panic!("an update call");
        };
        assert_eq!(id, "ev_1");
        assert_eq!(patch.end, Some(None));
        assert_eq!(patch.reminder_minutes, Some(None));
    }

    #[test]
    fn an_update_that_changes_nothing_is_refused() {
        assert!(update(&request("calendar.update", json!({ "id": "ev_1" }))).is_err());
    }

    #[test]
    fn a_repeat_rule_is_read_and_an_empty_one_removes_it() {
        let weekly = create(&request(
            "calendar.create",
            json!({ "title": "Clase", "start": "2026-11-02T09:00", "recurrence": "FREQ=WEEKLY;BYDAY=MO" }),
        ))
        .expect("series");
        let CalendarCall::Create(new) = weekly else {
            panic!("a create call");
        };
        assert_eq!(
            new.recurrence.map(|rule| rule.to_string()).as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=1;BYDAY=MO")
        );
        let cleared = update(&request(
            "calendar.update",
            json!({ "id": "ev_1", "recurrence": "" }),
        ))
        .expect("update");
        let CalendarCall::Update { patch, .. } = cleared else {
            panic!("an update call");
        };
        assert_eq!(patch.recurrence, Some(None));
    }

    #[test]
    fn an_unsupported_and_a_broken_rule_get_their_own_codes() {
        let hourly =
            json!({ "title": "x", "start": "2026-11-02T09:00", "recurrence": "FREQ=HOURLY" });
        let error = create(&request("calendar.create", hourly)).expect_err("hourly");
        assert_eq!(error.code, "unsupported_recurrence");
        let broken =
            json!({ "title": "x", "start": "2026-11-02T09:00", "recurrence": "FREQ=DAILY;BOGUS" });
        let error = create(&request("calendar.create", broken)).expect_err("broken");
        assert_eq!(error.code, "invalid_recurrence");
    }
}
