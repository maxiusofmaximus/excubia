//! HTTP mapping for the chat routes: query parsing, body validation and the
//! `docs/contracts/errors.md` shapes. The repository itself knows no HTTP.

use std::collections::HashMap;

use axum::response::IntoResponse;
use serde::Deserialize;

pub use hn_core_chat::MAX_MESSAGE_BYTES;
use hn_core_chat::{message_text as validate_text, ChatError, TextError};

use crate::error::ApiError;

const MAX_TITLE_CHARS: usize = 200;
const MAX_SECTION_CHARS: usize = 64;
const MAX_CLIENT_ID_CHARS: usize = 128;

pub fn map_error(error: ChatError) -> ApiError {
    match error {
        ChatError::NoSuchConversation => ApiError::not_found("no_such_conversation"),
        ChatError::ConversationArchived => ApiError::refused("conversation_archived"),
        ChatError::NoSuchRun(_) | ChatError::RunNotActive(_) => ApiError::refused("run_closed"),
        ChatError::NoSuchSeat(agent_id) => ApiError::invalid_field(
            "unknown_agent",
            "agent_id",
            format!("unknown seat {agent_id:?}"),
        ),
        ChatError::NoSuchDelegation(_) => ApiError::not_found("no_such_delegation"),
        ChatError::DelegationNotActive(_) => ApiError::refused("delegation_cancelled"),
        ChatError::NoSuchApproval => ApiError::not_found("no_such_approval"),
        ChatError::ApprovalExpired => ApiError::refused("approval_expired"),
        ChatError::ApprovalAlreadyDecided => ApiError::refused("approval_already_decided"),
        ChatError::ApprovalMismatch => ApiError::refused("approval_mismatch"),
        ChatError::ApprovalDigestReused => ApiError::refused("approval_digest_reused"),
        ChatError::ApprovalNotApproved => ApiError::refused("approval_not_approved"),
        ChatError::ApprovalLimit => ApiError::refused("approval_limit"),
        ChatError::ApprovalRaced => ApiError::refused("approval_raced"),
        ChatError::ApprovalScopeDenied => ApiError::forbidden_code("approval_scope_denied"),
        ChatError::ApprovalLevelMismatch => ApiError::refused("approval_level_mismatch"),
        ChatError::ApprovalNotDelegable => ApiError::refused("approval_not_delegable"),
        ChatError::ApprovalSelfApproval => ApiError::refused("approval_self_approval"),
        ChatError::ApprovalBudget => ApiError::refused("approval_budget"),
        ChatError::HookScopeMismatch => ApiError::forbidden_code("hook_scope_mismatch"),
        ChatError::LeadExists(_) => ApiError::refused("lead_exists"),
        ChatError::ProjectExists => ApiError::refused("project_exists"),
        ChatError::SeatLimit => ApiError::refused("seat_limit"),
        ChatError::AssigneeBusy | ChatError::ActorBusy => ApiError::agent_busy(),
        ChatError::ScopeDenied => ApiError::refused("scope_denied"),
        ChatError::DepthExceeded => ApiError::refused("depth_exceeded"),
        ChatError::Cycle => ApiError::refused("cycle_detected"),
        ChatError::TurnBudget => ApiError::refused("turn_budget"),
        ChatError::HourBudget => ApiError::refused("hour_budget"),
        ChatError::UnknownProject(project_id) => ApiError::invalid_field(
            "unknown_project",
            "project_id",
            format!("unknown project {project_id:?}"),
        ),
        ChatError::UnknownSection(section_id) => ApiError::invalid_field(
            "unknown_section",
            "section_id",
            format!("unknown section {section_id:?}"),
        ),
        ChatError::InvalidCursor(cursor) => {
            ApiError::action("invalid_cursor", format!("invalid cursor {cursor:?}"))
        }
        ChatError::NotInitialised => ApiError::internal("chat database is not initialised"),
        ChatError::Inconsistent(detail) => ApiError::internal(detail),
        ChatError::Database(error) => ApiError::internal(error),
        ChatError::StoredEvent(error) => ApiError::internal(error),
    }
}

pub fn map_section_error(error: ChatError) -> ApiError {
    match error {
        ChatError::UnknownSection(section_id) => ApiError::invalid_field(
            "unknown_section",
            "id",
            format!("unknown section {section_id:?}"),
        ),
        other => map_error(other),
    }
}

pub fn map_ok<T: serde::Serialize>(field: &str, value: T) -> axum::response::Response {
    match serde_json::to_value(value) {
        Ok(value) => {
            let mut object = serde_json::Map::new();
            object.insert("ok".to_string(), serde_json::Value::Bool(true));
            object.insert(field.to_string(), value);
            axum::Json(serde_json::Value::Object(object)).into_response()
        }
        Err(error) => ApiError::internal(error).into_response(),
    }
}

/// Merge `ok: true` into an object that is already the response shape
/// (`bootstrap`, pages): the contract publishes the fields at the top level,
/// not nested under a field.
pub fn ok_object<T: serde::Serialize>(value: T) -> axum::response::Response {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(mut object)) => {
            object.insert("ok".to_string(), serde_json::Value::Bool(true));
            axum::Json(serde_json::Value::Object(object)).into_response()
        }
        Ok(_) => ApiError::internal("chat response is not an object").into_response(),
        Err(error) => ApiError::internal(error).into_response(),
    }
}

pub fn limit(
    query: &HashMap<String, String>,
    default: usize,
    maximum: usize,
) -> Result<usize, ApiError> {
    let Some(raw) = query.get("limit") else {
        return Ok(default);
    };
    let parsed: usize = raw
        .parse()
        .map_err(|_| ApiError::invalid_field("invalid_body", "limit", "limit must be a number"))?;
    Ok(parsed.clamp(1, maximum))
}

pub fn seq(query: &HashMap<String, String>, name: &str) -> Result<Option<i64>, ApiError> {
    let Some(raw) = query.get(name) else {
        return Ok(None);
    };
    match raw.parse::<i64>() {
        Ok(value) if value >= 0 => Ok(Some(value)),
        _ => Err(ApiError::action(
            "invalid_cursor",
            format!("{name} must be a non-negative integer"),
        )),
    }
}

pub fn flag(query: &HashMap<String, String>, name: &str) -> Result<bool, ApiError> {
    match query.get(name).map(String::as_str) {
        None | Some("false") | Some("0") => Ok(false),
        Some("true") | Some("1") => Ok(true),
        Some(_) => Err(ApiError::invalid_field(
            "invalid_body",
            name,
            format!("{name} must be true or false"),
        )),
    }
}

pub fn optional_title(raw: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let title = raw.trim();
    if title.is_empty() {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "title",
            "title cannot be blank",
        ));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "title",
            format!("title cannot exceed {MAX_TITLE_CHARS} characters"),
        ));
    }
    if title.chars().any(char::is_control) {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "title",
            "title cannot contain control characters",
        ));
    }
    Ok(Some(title.to_string()))
}

pub fn section_name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "name",
            "name is required",
        ));
    }
    if name.chars().count() > MAX_SECTION_CHARS {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "name",
            format!("name cannot exceed {MAX_SECTION_CHARS} characters"),
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "name",
            "name cannot contain control characters",
        ));
    }
    Ok(name.to_string())
}

pub fn client_id(raw: &str) -> Result<(), ApiError> {
    let valid = !raw.is_empty()
        && raw.chars().count() <= MAX_CLIENT_ID_CHARS
        && !raw.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(ApiError::invalid_field(
            "invalid_body",
            "client_id",
            format!("client_id must be 1..={MAX_CLIENT_ID_CHARS} characters without controls"),
        ))
    }
}

pub fn message_text(raw: &str) -> Result<String, ApiError> {
    validate_text(raw).map_err(|error| match error {
        TextError::Empty => ApiError::action("empty_message", "text is required"),
        TextError::ControlCharacters => {
            ApiError::action("control_characters", "message contains control characters")
        }
        TextError::TooLong => ApiError::invalid_field(
            "invalid_body",
            "text",
            format!("message cannot exceed {MAX_MESSAGE_BYTES} bytes"),
        ),
    })
}

pub fn read_seq(raw: i64) -> Result<i64, ApiError> {
    if raw < 0 {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "last_seq",
            "last_seq must be a non-negative integer",
        ));
    }
    Ok(raw)
}

/// `section_id: null` clears the section; an absent field keeps it. Serde's
/// plain `Option<Option<T>>` collapses both, so the field is wrapped here.
pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn message_text_is_bounded_and_clean() {
        assert_eq!(message_text("  hola  ").expect("text"), "hola");
        assert_eq!(
            message_text("   ").expect_err("empty").body["detail"]["code"],
            "empty_message"
        );
        assert_eq!(
            message_text("a\u{7}").expect_err("control").body["detail"]["code"],
            "control_characters"
        );
        let long = "a".repeat(MAX_MESSAGE_BYTES + 1);
        let error = message_text(&long).expect_err("too long");
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error.body["detail"]["field"], "text");
    }

    #[test]
    fn titles_sections_and_client_ids_follow_the_model() {
        assert_eq!(
            optional_title(Some("  plan  ".into())).expect("title"),
            Some("plan".to_string())
        );
        assert!(optional_title(Some("   ".into())).is_err());
        assert!(optional_title(Some("a\nb".into())).is_err());
        assert_eq!(section_name("  Trabajo ").expect("section"), "Trabajo");
        assert!(section_name("").is_err());
        assert!(client_id("client-1").is_ok());
        assert!(client_id("").is_err());
    }

    #[test]
    fn query_flags_and_limits_are_typed() {
        let mut query = HashMap::new();
        assert!(!flag(&query, "pinned").expect("default"));
        query.insert("pinned".into(), "true".into());
        assert!(flag(&query, "pinned").expect("true"));
        query.insert("pinned".into(), "yes".into());
        assert!(flag(&query, "pinned").is_err());
        query.insert("limit".into(), "500".into());
        assert_eq!(limit(&query, 30, 100).expect("clamp"), 100);
        query.insert("limit".into(), "nope".into());
        assert!(limit(&query, 30, 100).is_err());
        query.insert("before_seq".into(), "-1".into());
        assert!(seq(&query, "before_seq").is_err());
    }
}
