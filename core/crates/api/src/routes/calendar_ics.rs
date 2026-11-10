//! `/v1/calendar/import` and `/v1/calendar/export.ics` (CALENDARIO-F4c): the calendar as an
//! iCalendar file. The reading and the writing are `excubia-core-chat`'s; a handler here shapes the
//! request and the reply. The permissions are in `excubia-core-auth`'s `authz/agenda.rs`.

use axum::body::Bytes;
use axum::extract::{Extension, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{export_calendar, read_calendar, EventTime, IcsError};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::routes::calendar::map_calendar_error;
use crate::ApiContext;

pub const IMPORT_PATH: &str = "/v1/calendar/import";
/// A file of 500 events fits well inside this; a larger body is refused before it is read.
pub const MAX_ICS_BODY_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
pub struct ExportQuery {
    from: Option<String>,
    to: Option<String>,
}

/// The events of the window, as a file to save. Without bounds, all of them.
pub async fn export(
    State(ctx): State<ApiContext>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let from = bound("from", query.from)?;
    let to = bound("to", query.to)?;
    let events = ctx
        .chat
        .export_events(from, to)
        .map_err(map_calendar_error)?;
    let text = export_calendar(&events).map_err(ApiError::internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"excubia.ics\"",
            ),
        ],
        text,
    )
        .into_response())
}

/// The file's events, each kept or named with the reason it was not. The middleware reads the
/// body under `MAX_ICS_BODY_BYTES`.
pub async fn import(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let parsed = read_calendar(&body).map_err(map_ics_error)?;
    let report = ctx
        .chat
        .import_events(&verified.device_id, parsed)
        .map_err(map_calendar_error)?;
    Ok(Json(json!({
        "ok": true,
        "imported": report.imported,
        "skipped_existing": report.skipped_existing,
        "skipped_unsupported": report.skipped_unsupported,
        "details": report.details,
    })))
}

/// A bound of the export, read as the calendar reads a time; a text that is none is refused.
fn bound(field: &'static str, text: Option<String>) -> Result<Option<EventTime>, ApiError> {
    text.map(|text| {
        EventTime::parse(&text).ok_or_else(|| {
            ApiError::invalid_field(
                "invalid_field",
                field,
                "a time is a date (2026-11-03), a local time (2026-11-03T09:30) or one with an offset",
            )
        })
    })
    .transpose()
}

fn map_ics_error(error: IcsError) -> ApiError {
    match error {
        IcsError::Invalid => ApiError::coded(
            StatusCode::BAD_REQUEST,
            "invalid_ics",
            "the file is not an iCalendar file",
        ),
        IcsError::TooManyEvents => ApiError::coded(
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_events",
            "the file holds more than 500 events: split it and import the parts",
        ),
    }
}
