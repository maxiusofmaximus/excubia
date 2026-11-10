//! The REST side of the agenda (AGENDA-REST): the tool parsers check every field the way
//! they check Alfred's, so the limits and codes are the same, and one mapping turns the
//! store's refusals into the shapes `docs/contracts/errors.md` gives them.

use axum::http::StatusCode;
use excubia_core_chat::{ChatError, ToolError, ToolRequest};
use excubia_core_seat::{parse_tool, ToolCall};
use serde_json::{json, Value};

use crate::error::ApiError;

/// The page a list returns when the caller names no limit, and the most a page may hold.
pub(crate) const DEFAULT_PAGE: usize = 20;
pub(crate) const MAX_PAGE: usize = 100;

/// Runs the tool parser on the arguments a REST body names.
pub(crate) fn parse(tool: &str, arguments: Value) -> Result<ToolCall, ToolError> {
    parse_tool(&ToolRequest {
        tool: tool.to_string(),
        arguments,
    })
}

impl From<ToolError> for ApiError {
    fn from(error: ToolError) -> Self {
        match error.field {
            Some(field) => ApiError::invalid_field(error.code, field, error.message),
            None => ApiError::coded(StatusCode::UNPROCESSABLE_ENTITY, error.code, error.message),
        }
    }
}

/// The page size a list names: `1` to `MAX_PAGE`, `DEFAULT_PAGE` when absent.
pub(crate) fn page_limit(raw: Option<usize>) -> Result<usize, ApiError> {
    let limit = raw.unwrap_or(DEFAULT_PAGE);
    if (1..=MAX_PAGE).contains(&limit) {
        return Ok(limit);
    }
    Err(ApiError::invalid_field(
        "invalid_field",
        "limit",
        format!("limit must be 1 to {MAX_PAGE}"),
    ))
}

/// The cursor of the next page: the last row of a full page, `null` on the last page.
pub(crate) fn next_cursor(last_id: Option<&str>, count: usize, limit: usize) -> Value {
    match last_id {
        Some(id) if count == limit => json!(id),
        _ => Value::Null,
    }
}

/// The store's refusals of the agenda. Anything the agenda's calls cannot refuse is a
/// fault: its text stays in the log.
pub(crate) fn map_error(error: ChatError) -> ApiError {
    match error {
        ChatError::NoteNotFound(_) | ChatError::ContactNotFound(_) => ApiError::coded(
            StatusCode::NOT_FOUND,
            "not_found",
            "no such note or contact",
        ),
        ChatError::NoteLimit => ApiError::coded(
            StatusCode::CONFLICT,
            "note_limit",
            "the company already keeps the most notes it can",
        ),
        ChatError::ContactLimit => ApiError::coded(
            StatusCode::CONFLICT,
            "contact_limit",
            "the company already keeps the most contacts it can",
        ),
        ChatError::AddressTaken(name) => ApiError::coded(
            StatusCode::CONFLICT,
            "address_taken",
            format!("the address already belongs to the contact {name}"),
        ),
        ChatError::AliasTaken => ApiError::coded(
            StatusCode::CONFLICT,
            "alias_taken",
            "the alias already names another contact",
        ),
        ChatError::AddressLimit => ApiError::invalid_field(
            "address_limit",
            "addresses",
            "a contact keeps too many addresses",
        ),
        ChatError::AliasLimit => {
            ApiError::invalid_field("alias_limit", "aliases", "a contact keeps too many aliases")
        }
        ChatError::InvalidAddress => ApiError::invalid_field(
            "invalid_address",
            "addresses",
            "an address must be one local@domain.tld without spaces",
        ),
        ChatError::NoSuchMessage(_) => {
            ApiError::invalid_field("unknown_message", "message_id", "no such message")
        }
        other => ApiError::internal(other),
    }
}
