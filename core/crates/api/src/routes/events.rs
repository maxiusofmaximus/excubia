//! `GET /v1/events`: the live state over SSE, plus the durable chat frames.
//!
//! `Last-Event-ID` resumes only the chat journal: state frames are derived and
//! the opening frame already carries the full state, so replaying them would
//! duplicate work. The header is the SSE id a chat frame carried.

use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::http::{header, HeaderMap, HeaderName};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use hn_core_auth::VerifiedDevice;
use hn_core_chat::{ChatError, ChatFrame, ChatStore};
use serde_json::json;
use tokio::sync::mpsc;

use crate::error::ApiError;
use crate::sse::{spawn_forwarder, EventStream, Forwarder};
use crate::ApiContext;

const REPLAY_PAGE: usize = 1_000;
const REPLAY_MAX: usize = 10_000;

pub async fn events(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    headers: HeaderMap,
) -> Response {
    let last_id = match last_event_id(&headers) {
        Ok(last_id) => last_id,
        Err(error) => return error.into_response(),
    };
    let (payload, frames) = ctx.store.subscribe().await;
    // Subscribe before reading the journal: any frame published while the
    // replay query runs is then either in the replay or in this receiver.
    let chat = ctx.chat.subscribe();
    let replay = match last_id {
        Some(last_id) => match replay_chat(&ctx.chat, last_id) {
            Ok(replay) => replay,
            Err(error) => return ApiError::internal(error).into_response(),
        },
        None => Vec::new(),
    };
    let revision = payload.revision;
    let mut state = match serde_json::to_value(&payload) {
        Ok(state) => state,
        Err(error) => return ApiError::internal(error).into_response(),
    };
    match super::chat::state_block(&ctx.chat) {
        Ok(chat) => state["chat"] = chat,
        Err(error) => return ApiError::internal(error).into_response(),
    }
    // Acquire only once every fallible part is done: a slot must be owned by
    // the stream guard that releases it, never leaked by an early return.
    if !ctx.streams.acquire(&verified.device_id) {
        return ApiError::too_many_streams().into_response();
    }
    let first = json!({
        "event": "state",
        "revision": revision,
        "state": state,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_forwarder(Forwarder {
        frames,
        chat,
        replay,
        revocations: ctx.revocations.subscribe(),
        devices: Arc::clone(&ctx.devices),
        device_id: verified.device_id.clone(),
        tx,
    });
    let stream = EventStream::new(
        Event::default().event("state").data(first.to_string()),
        rx,
        Arc::clone(&ctx.streams),
        verified.device_id.clone(),
    );
    let sse = Sse::new(stream);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        sse,
    )
        .into_response()
}

fn last_event_id(headers: &HeaderMap) -> Result<Option<i64>, ApiError> {
    let Some(value) = headers.get("last-event-id") else {
        return Ok(None);
    };
    let raw = value.to_str().map_err(|_| invalid_last_event_id())?;
    let id: i64 = raw.trim().parse().map_err(|_| invalid_last_event_id())?;
    if id < 0 {
        return Err(invalid_last_event_id());
    }
    Ok(Some(id))
}

fn invalid_last_event_id() -> ApiError {
    ApiError::action(
        "invalid_cursor",
        "Last-Event-ID must be a non-negative integer",
    )
}

/// Journal frames after `last_id`, in pages, bounded so one reconnect cannot
/// hold the database forever. If the bound is hit the client still has its
/// full timeline read (`GET .../timeline`) as the authoritative resync.
fn replay_chat(chat: &ChatStore, last_id: i64) -> Result<Vec<ChatFrame>, ChatError> {
    let mut frames = Vec::new();
    let mut cursor = last_id;
    while frames.len() < REPLAY_MAX {
        let page = chat.events_after(cursor, REPLAY_PAGE)?;
        let exhausted = page.len() < REPLAY_PAGE;
        if let Some(last) = page.last() {
            cursor = last.id;
        }
        frames.extend(page);
        if exhausted {
            break;
        }
    }
    Ok(frames)
}
