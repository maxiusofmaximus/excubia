//! Conversations: list/search, create, detail and metadata updates.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use hn_core_chat::{ConversationFilter, ConversationUpdate, NewConversation, SectionChange};

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support::{self, double_option, flag, limit, optional_title};

#[derive(Deserialize, Default)]
struct CreateBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Deserialize, Default)]
struct UpdateBody {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    pinned: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    section_id: Option<Option<String>>,
    #[serde(default)]
    archived: Option<bool>,
}

pub async fn list(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let filter = match filter(&query) {
        Ok(filter) => filter,
        Err(error) => return error.into_response(),
    };
    match ctx.chat.list_conversations(filter) {
        Ok(page) => support::ok_object(page),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn create(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: CreateBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let title = match optional_title(payload.title) {
        Ok(title) => title,
        Err(error) => return error.into_response(),
    };
    let new = NewConversation {
        title,
        project_id: payload.project_id.filter(|id| !id.trim().is_empty()),
    };
    match ctx.chat.create_conversation(new) {
        Ok(conversation) => support::map_ok("conversation", conversation),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn show(State(ctx): State<ApiContext>, Path(conversation_id): Path<String>) -> Response {
    match ctx.chat.conversation(&conversation_id) {
        Ok(detail) => support::ok_object(detail),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn update(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: UpdateBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let title = match optional_title(payload.title) {
        Ok(title) => title,
        Err(error) => return error.into_response(),
    };
    let section = match payload.section_id {
        None => SectionChange::Keep,
        Some(None) => SectionChange::Clear,
        Some(Some(section_id)) => SectionChange::Set(section_id),
    };
    let update = ConversationUpdate {
        title,
        pinned: payload.pinned,
        section,
        archived: payload.archived,
    };
    match ctx.chat.update_conversation(&conversation_id, update) {
        Ok(conversation) => support::map_ok("conversation", conversation),
        Err(error) => support::map_error(error).into_response(),
    }
}

fn filter(query: &HashMap<String, String>) -> Result<ConversationFilter, ApiError> {
    let q = query
        .get("q")
        .map(|q| q.trim())
        .filter(|q| !q.is_empty())
        .map(str::to_string);
    let section_id = query.get("section_id").filter(|id| !id.is_empty()).cloned();
    let cursor = query.get("cursor").filter(|id| !id.is_empty()).cloned();
    Ok(ConversationFilter {
        q,
        section_id,
        pinned: flag(query, "pinned")?,
        archived: flag(query, "archived")?,
        cursor,
        limit: limit(query, 30, 100)?,
    })
}
