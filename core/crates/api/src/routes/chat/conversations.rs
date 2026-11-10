//! Conversations: list/search, create, detail and metadata updates.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use serde::Deserialize;
use serde_json::json;

use excubia_core_auth::VerifiedDevice;
use excubia_core_chat::{ConversationFilter, ConversationUpdate, NewConversation, SectionChange};

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

/// `GET /v1/chat/conversations/counts`: the chip numbers of the list, one query.
pub async fn counts(State(ctx): State<ApiContext>) -> Response {
    match ctx.chat.conversation_counts() {
        Ok(counts) => support::ok_object(counts),
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

/// `DELETE /v1/chat/conversations/{id}` (signed, `RESPOND`): the user deletes the
/// conversation for good. The device that asks is the one the audit names.
pub async fn delete_conversation(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(conversation_id): Path<String>,
) -> Response {
    match ctx
        .chat
        .delete_conversation(&conversation_id, &verified.device_id)
    {
        Ok(()) => support::ok_object(json!({})),
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
        attention: only_flag(query, "attention")?,
        working: only_flag(query, "working")?,
        cursor,
        limit: limit(query, 30, 100)?,
    })
}

/// `attention` and `working` take exactly `1` or `true`; absent means off.
fn only_flag(query: &HashMap<String, String>, name: &str) -> Result<bool, ApiError> {
    match query.get(name).map(String::as_str) {
        None => Ok(false),
        Some("1" | "true") => Ok(true),
        Some(_) => Err(ApiError::invalid_field(
            "invalid_body",
            name,
            format!("{name} must be 1 or true"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn filter_flags_accept_only_one_or_true() {
        let mut query = HashMap::new();
        assert!(!only_flag(&query, "working").expect("absent"));
        query.insert("working".into(), "1".into());
        assert!(only_flag(&query, "working").expect("1"));
        query.insert("working".into(), "true".into());
        assert!(only_flag(&query, "working").expect("true"));
        for refused in ["0", "false", "yes", ""] {
            query.insert("working".into(), refused.into());
            let error = only_flag(&query, "working").expect_err(refused);
            assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(error.body["detail"]["field"], "working");
        }
    }
}
