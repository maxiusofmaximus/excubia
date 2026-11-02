//! The calendar over REST: the app's events and the company's zone (`/v1/calendar`,
//! `/v1/company/timezone`). Reads need `READ_STATE`, writes `RESPOND`, deleting an
//! event and fixing the zone `APPROVE`. Every call is signed like the rest of the chat routes.

use reqwest::Method;
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::calendar_model::{CalendarEvent, CompanyTimezone, EventDraft, EventPage};
use super::error::ChatResult;
use super::validate_prefixed_id;
use crate::client::HostClient;

const EVENT_PREFIX: &str = "ev_";
const EVENT_LABEL: &str = "evento";
/// The page the screen asks for: the host's default, written out.
const EVENT_PAGE_LIMIT: u16 = 20;

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    event: CalendarEvent,
}

/// `/v1/calendar/events/{id}{action}`, with the id checked first.
fn event_path(id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, EVENT_PREFIX, EVENT_LABEL)?;
    Ok(format!("/v1/calendar/events/{id}{action}"))
}

/// The fields every write carries. Empty text is sent as it is: the host stores it.
fn event_body(draft: &EventDraft) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert("title".to_string(), json!(draft.title));
    body.insert("start".to_string(), json!(draft.start));
    body.insert("all_day".to_string(), json!(draft.all_day));
    body.insert("location".to_string(), json!(draft.location));
    body.insert("notes".to_string(), json!(draft.notes));
    body
}

/// A new event: `end`, the reminder and the rule are left out when the draft has none,
/// since the host refuses an empty end on create.
fn create_body(draft: &EventDraft) -> Value {
    let mut body = event_body(draft);
    if let Some(end) = &draft.end {
        body.insert("end".to_string(), json!(end));
    }
    if let Some(minutes) = draft.reminder_minutes {
        body.insert("reminder_minutes".to_string(), json!(minutes));
    }
    if let Some(rule) = &draft.recurrence {
        body.insert("recurrence".to_string(), json!(rule));
    }
    Value::Object(body)
}

/// An edit replaces the whole event: no end is `""`, no reminder is `-1` and no rule
/// is `""`, which is how the host removes them.
fn update_body(draft: &EventDraft) -> Value {
    let mut body = event_body(draft);
    body.insert(
        "end".to_string(),
        json!(draft.end.as_deref().unwrap_or_default()),
    );
    body.insert(
        "reminder_minutes".to_string(),
        json!(draft.reminder_minutes.unwrap_or(-1)),
    );
    body.insert(
        "recurrence".to_string(),
        json!(draft.recurrence.as_deref().unwrap_or_default()),
    );
    Value::Object(body)
}

/// The query of a page. A cursor is an event id, or `id@occurrence_start` for an
/// occurrence; the id is checked here and the occurrence by the host.
fn page_query(
    from: &str,
    to: &str,
    after: Option<&str>,
) -> ChatResult<Vec<(&'static str, String)>> {
    let mut query = vec![
        ("from", from.to_string()),
        ("to", to.to_string()),
        ("limit", EVENT_PAGE_LIMIT.to_string()),
    ];
    if let Some(cursor) = after {
        validate_prefixed_id(
            cursor.split_once('@').map_or(cursor, |(id, _)| id),
            EVENT_PREFIX,
            EVENT_LABEL,
        )?;
        query.push(("after", cursor.to_string()));
    }
    Ok(query)
}

impl HostClient {
    pub async fn calendar_timezone(&self) -> ChatResult<CompanyTimezone> {
        self.chat_get("/v1/company/timezone", &[]).await
    }

    pub async fn calendar_set_timezone(&self, timezone: &str) -> ChatResult<CompanyTimezone> {
        let body = json!({ "timezone": timezone });
        self.chat_send_json(Method::POST, "/v1/company/timezone", &[], &body, true)
            .await
    }

    /// The events from `from` up to `to` (not included), earliest first.
    pub async fn calendar_events(
        &self,
        from: &str,
        to: &str,
        after: Option<&str>,
    ) -> ChatResult<EventPage> {
        let query = page_query(from, to, after)?;
        self.chat_get("/v1/calendar/events", &query).await
    }

    /// One event as the host keeps it: a series with its own start and rule.
    pub async fn calendar_get(&self, id: &str) -> ChatResult<CalendarEvent> {
        let path = event_path(id, "")?;
        let envelope: EventEnvelope = self.chat_get(&path, &[]).await?;
        Ok(envelope.event)
    }

    pub async fn calendar_create(&self, draft: &EventDraft) -> ChatResult<CalendarEvent> {
        let envelope: EventEnvelope = self
            .chat_send_json(
                Method::POST,
                "/v1/calendar/events",
                &[],
                &create_body(draft),
                true,
            )
            .await?;
        Ok(envelope.event)
    }

    pub async fn calendar_update(&self, id: &str, draft: &EventDraft) -> ChatResult<CalendarEvent> {
        let path = event_path(id, "/update")?;
        let envelope: EventEnvelope = self
            .chat_send_json(Method::POST, &path, &[], &update_body(draft), true)
            .await?;
        Ok(envelope.event)
    }

    pub async fn calendar_delete(&self, id: &str) -> ChatResult<()> {
        let path = event_path(id, "/delete")?;
        self.chat_send_json::<IgnoredAny>(Method::POST, &path, &[], &json!({}), true)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;
    use crate::chat::error::{status_failure, ChatFailure};

    fn draft(end: Option<&str>, reminder: Option<i64>) -> EventDraft {
        EventDraft {
            title: "Llamada con Ana".to_string(),
            start: "2026-11-03T09:30".to_string(),
            end: end.map(str::to_string),
            all_day: false,
            location: String::new(),
            notes: String::new(),
            reminder_minutes: reminder,
            recurrence: None,
        }
    }

    fn series(rule: Option<&str>) -> EventDraft {
        EventDraft {
            recurrence: rule.map(str::to_string),
            ..draft(None, None)
        }
    }

    #[test]
    fn a_new_event_leaves_out_the_end_and_the_reminder_it_lacks() {
        let body = create_body(&draft(None, None));
        assert!(body.get("end").is_none());
        assert!(body.get("reminder_minutes").is_none());
        assert_eq!(body["start"], "2026-11-03T09:30");
    }

    #[test]
    fn a_new_event_sends_the_end_and_the_reminder_it_has() {
        let body = create_body(&draft(Some("2026-11-03T10:00"), Some(15)));
        assert_eq!(body["end"], "2026-11-03T10:00");
        assert_eq!(body["reminder_minutes"], 15);
    }

    #[test]
    fn an_edit_without_end_or_reminder_clears_both_on_the_host() {
        let body = update_body(&draft(None, None));
        assert_eq!(body["end"], "");
        assert_eq!(body["reminder_minutes"], -1);
    }

    #[test]
    fn an_event_route_checks_its_id() {
        assert!(event_path("ct_1", "/update").is_err());
        assert_eq!(
            event_path("ev_1", "/delete").unwrap(),
            "/v1/calendar/events/ev_1/delete"
        );
    }

    #[test]
    fn a_series_sends_its_rule_and_a_new_event_without_one_sends_none() {
        let body = create_body(&series(Some("FREQ=WEEKLY;INTERVAL=1;BYDAY=MO")));
        assert_eq!(body["recurrence"], "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO");
        assert!(create_body(&series(None)).get("recurrence").is_none());
    }

    #[test]
    fn an_edit_without_a_rule_removes_the_series_on_the_host() {
        assert_eq!(update_body(&series(None))["recurrence"], "");
        assert_eq!(
            update_body(&series(Some("FREQ=DAILY;INTERVAL=2")))["recurrence"],
            "FREQ=DAILY;INTERVAL=2"
        );
    }

    #[test]
    fn a_page_cursor_may_name_an_occurrence_and_keeps_the_window() {
        let query = page_query(
            "2026-11-01",
            "2026-12-01",
            Some("ev_1@2026-11-04T08:30:00.000Z"),
        )
        .unwrap();
        assert_eq!(query.last().unwrap().0, "after");
        assert_eq!(query.last().unwrap().1, "ev_1@2026-11-04T08:30:00.000Z");
        assert!(page_query("2026-11-01", "2026-12-01", Some("ct_1@2026-11-04")).is_err());
    }

    #[test]
    fn a_truncated_page_carries_its_occurrences_and_the_cursor_of_the_next() {
        let page: EventPage = serde_json::from_str(
            r#"{"events":[{"id":"ev_1","title":"Tutoría","start_utc":"2026-11-04T08:30:00.000Z",
                "occurrence_start":"2026-11-04T08:30:00.000Z","recurrence":"FREQ=WEEKLY;INTERVAL=1;BYDAY=WE",
                "recurring":true}],"truncated":true,"next":"ev_1@2026-11-04T08:30:00.000Z"}"#,
        )
        .unwrap();
        assert!(page.truncated);
        assert_eq!(page.next.as_deref(), Some("ev_1@2026-11-04T08:30:00.000Z"));
        assert!(page.events[0].recurring);
        assert_eq!(
            page.events[0].occurrence_start.as_deref(),
            Some("2026-11-04T08:30:00.000Z")
        );
    }

    #[test]
    fn a_recurrence_refusal_keeps_its_code_and_a_bad_field_names_recurrence() {
        let unsupported =
            br#"{"detail":{"code":"unsupported_recurrence","message":"FREQ=HOURLY"}}"#;
        match status_failure(StatusCode::UNPROCESSABLE_ENTITY, unsupported, None, true) {
            ChatFailure::AgendaRefused { code, .. } => assert_eq!(code, "unsupported_recurrence"),
            other => panic!("esperaba agenda, llegó {other:?}"),
        }
        let field =
            br#"{"detail":{"code":"invalid_field","field":"recurrence","message":"UNTIL"}}"#;
        match status_failure(StatusCode::UNPROCESSABLE_ENTITY, field, None, true) {
            ChatFailure::Invalid { field, .. } => assert_eq!(field.as_deref(), Some("recurrence")),
            other => panic!("esperaba invalid, llegó {other:?}"),
        }
    }
}
