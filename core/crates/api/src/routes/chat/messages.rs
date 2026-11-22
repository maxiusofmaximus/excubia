//! Messages: the idempotent user turn, timeline pagination, raw sync and the
//! read cursor.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use excubia_chat::AcceptedMessage;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support::{self, client_id, limit, message_text_with_files, read_seq, seq};

#[derive(Deserialize)]
struct SendBody {
    client_id: String,
    text: String,
    reply_to: Option<ReplyTo>,
    /// Ready attachments of this conversation, 0 to 10. Absent for a client that predates them.
    #[serde(default)]
    attachments: Vec<String>,
}

#[derive(Deserialize)]
struct ReplyTo {
    kind: String,
    id: String,
}

#[derive(Deserialize)]
struct ReadBody {
    last_seq: i64,
}

pub async fn timeline(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let before_seq = match seq(&query, "before_seq") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let limit = match limit(&query, 50, 200) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match ctx.chat.timeline(&conversation_id, before_seq, limit) {
        Ok(page) => {
            let mut body = match serde_json::to_value(page) {
                Ok(body) => body,
                Err(error) => return ApiError::internal(error).into_response(),
            };
            body["conversation_id"] = json!(conversation_id);
            support::ok_object(body)
        }
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn messages(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let after_seq = match seq(&query, "after_seq") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let before_seq = match seq(&query, "before_seq") {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let limit = match limit(&query, 100, 200) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match ctx
        .chat
        .messages(&conversation_id, after_seq, before_seq, limit)
    {
        Ok(page) => support::ok_object(page),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn send(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: SendBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = client_id(&payload.client_id) {
        return error.into_response();
    }
    let text = match message_text_with_files(&payload.text, payload.attachments.len()) {
        Ok(text) => text,
        Err(error) => return error.into_response(),
    };
    // v1 replies only to messages; the store checks that the id is in this conversation.
    let reply_to = match &payload.reply_to {
        Some(reply) if reply.kind != "message" => {
            return ApiError::invalid_field(
                "unsupported_reply_kind",
                "reply_to",
                "reply_to.kind must be \"message\"",
            )
            .into_response();
        }
        reply => reply
            .as_ref()
            .map(|reply| (reply.kind.as_str(), reply.id.as_str())),
    };
    let attachments: Vec<&str> = payload.attachments.iter().map(String::as_str).collect();
    match ctx.chat.append_user_message_with_attachments(
        &conversation_id,
        &payload.client_id,
        &text,
        reply_to,
        &attachments,
    ) {
        Ok(accepted) => accepted_response(&ctx, accepted),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn read(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: ReadBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let last_seq = match read_seq(payload.last_seq) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    match ctx.chat.mark_read(&conversation_id, last_seq) {
        Ok(last_read_seq) => support::ok_object(json!({
            "conversation_id": conversation_id,
            "last_read_seq": last_read_seq,
        })),
        Err(error) => support::map_error(error).into_response(),
    }
}

/// `202`: the message and its queued turn are durable. A duplicate `client_id`
/// returns the original turn and never enqueues it again.
fn accepted_response(ctx: &ApiContext, accepted: AcceptedMessage) -> Response {
    let AcceptedMessage {
        message,
        turn,
        duplicate,
    } = accepted;
    if !duplicate {
        if let Err(error) = ctx.seat.try_enqueue(turn.clone()) {
            // The turn was not delivered, so the message must not stay in the chat.
            if let Err(withdraw) = ctx.chat.withdraw_turn(&message.id) {
                eprintln!(
                    "chat: could not withdraw refused turn {}: {withdraw}",
                    message.id
                );
            }
            return super::seat_error(error);
        }
    }
    let body = json!({"ok": true, "message": message, "turn": turn});
    (StatusCode::ACCEPTED, Json(body)).into_response()
}
