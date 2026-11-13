//! `/v1/calendar` and `/v1/company/timezone` (CALENDARIO-F4): the events the app keeps and
//! the zone the company keeps them in. The matrix that guards them is `excubia-auth`'s
//! `authz/agenda.rs`; a handler here only shapes the request and the reply. A body goes
//! through the tool parser, so its limits and codes are Alfred's.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_auth::VerifiedDevice;
use excubia_chat::{CalendarError, ChatError, EventOrigin};
use excubia_seat::{CalendarCall, ToolCall};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::agenda_api::{map_error, page_limit, parse};
use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
pub struct ListQuery {
    from: String,
    to: String,
    limit: Option<usize>,
    after: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimezoneBody {
    timezone: String,
}

/// The events that start from `from` up to `to` (not included), earliest first.
pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page_limit(query.limit)?;
    let arguments =
        json!({ "from": query.from, "to": query.to, "limit": limit, "after": query.after });
    let CalendarCall::List(request) = calendar_call("calendar.list", arguments)? else {
        return Err(mismatch());
    };
    let window = ctx
        .chat
        .calendar_window(request.from, request.to)
        .map_err(map_calendar_error)?;
    let listing = ctx
        .chat
        .list_events(window, request.after.as_deref(), request.limit)
        .map_err(map_calendar_error)?;
    let next = match listing.truncated {
        true => listing.events.last().map(|entry| entry.cursor()),
        false => None,
    };
    Ok(Json(json!({
        "ok": true,
        "events": listing.events,
        "truncated": listing.truncated,
        "next": next,
    })))
}

pub async fn show(
    State(ctx): State<ApiContext>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let event = ctx.chat.event(&event_id).map_err(map_calendar_error)?;
    Ok(Json(json!({"ok": true, "event": event})))
}

pub async fn create(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: Map<String, Value> = parse_body(&body)?;
    let CalendarCall::Create(new) = calendar_call("calendar.create", Value::Object(payload))?
    else {
        return Err(mismatch());
    };
    let event = ctx
        .chat
        .create_event(EventOrigin::Device(&verified.device_id), new)
        .map_err(map_calendar_error)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"ok": true, "event": event})),
    )
        .into_response())
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(event_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let mut payload: Map<String, Value> = parse_body(&body)?;
    payload.insert("id".to_string(), json!(event_id));
    let CalendarCall::Update { id, patch } =
        calendar_call("calendar.update", Value::Object(payload))?
    else {
        return Err(mismatch());
    };
    let event = ctx
        .chat
        .update_event(EventOrigin::Device(&verified.device_id), &id, patch)
        .map_err(map_calendar_error)?;
    Ok(Json(json!({"ok": true, "event": event})))
}

/// The user's own act, so it runs at once; an event that is gone answers `404`.
pub async fn delete(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(event_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    ctx.chat
        .delete_event_by_device(&verified.device_id, &event_id)
        .map_err(map_calendar_error)?;
    Ok(Json(json!({"ok": true, "id": event_id})))
}

/// The zone the company keeps its calendar in; `null` until someone sets it.
pub async fn timezone(State(ctx): State<ApiContext>) -> Result<Json<Value>, ApiError> {
    let timezone = ctx.chat.company_timezone().map_err(map_calendar_error)?;
    Ok(Json(json!({"ok": true, "timezone": timezone})))
}

pub async fn set_timezone(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let payload: TimezoneBody = parse_body(&body)?;
    ctx.chat
        .set_company_timezone(&verified.device_id, &payload.timezone)
        .map_err(map_calendar_error)?;
    Ok(Json(json!({"ok": true, "timezone": payload.timezone})))
}

/// The calendar call a body names: the tool parser checks it as Alfred's would.
fn calendar_call(tool: &str, arguments: Value) -> Result<CalendarCall, ApiError> {
    match parse(tool, arguments)? {
        ToolCall::Calendar(call) => Ok(call),
        _ => Err(mismatch()),
    }
}

fn mismatch() -> ApiError {
    ApiError::internal("the calendar parser answered with another tool")
}

/// What the store refuses of the calendar, in the shapes `docs/contracts/errors.md` gives.
pub(super) fn map_calendar_error(error: CalendarError) -> ApiError {
    match error {
        CalendarError::TimezoneUnset => ApiError::coded(
            StatusCode::CONFLICT,
            "timezone_unset",
            "the company has no calendar zone yet",
        ),
        CalendarError::TimezoneAlreadySet => ApiError::coded(
            StatusCode::CONFLICT,
            "timezone_already_set",
            "the company's calendar zone is already set",
        ),
        CalendarError::InvalidZone => ApiError::invalid_field(
            "invalid_zone",
            "timezone",
            "a zone is an IANA name, such as Europe/Madrid",
        ),
        CalendarError::InvalidLocalTime => ApiError::invalid_field(
            "invalid_local_time",
            "start",
            "that clock time does not exist in the company's zone",
        ),
        CalendarError::InvalidField { field, message } => {
            ApiError::invalid_field("invalid_field", field, message)
        }
        CalendarError::EventLimit => ApiError::coded(
            StatusCode::CONFLICT,
            "calendar_limit",
            "the company already keeps the most events it can",
        ),
        CalendarError::TooManyEvents => ApiError::coded(
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_events",
            "the calendar holds more events than one file takes: narrow the window with from and to",
        ),
        CalendarError::EventNotFound(_) => {
            ApiError::coded(StatusCode::NOT_FOUND, "not_found", "no such event")
        }
        CalendarError::Chat(error) => map_error(error),
        CalendarError::Database(error) => ApiError::internal(ChatError::from(error)),
    }
}
