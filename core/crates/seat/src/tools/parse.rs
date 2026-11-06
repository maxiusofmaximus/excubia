//! Turns a tool request into a `ToolCall`, refusing anything malformed.

use hn_core_chat::{message_text, validate_suggestions, TextError, ToolError, ToolRequest};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::args::{
    AgentSpawnArgs, AskArgs, DelegationAssignArgs, DelegationCompleteArgs, ProgressArgs,
    ProjectCreateArgs, ReplyArgs,
};

use super::call::{AgentSpawn, DelegationAssign, DelegationComplete, ProjectCreate};

use super::{
    alert, approval, calendar, contact, decide, mail, note, pending, plan, status, ToolCall,
};

/// The longest progress line, in characters. Longer text is refused, never cut.
pub(super) const PROGRESS_MAX_CHARS: usize = 120;
pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    match request.tool.as_str() {
        "chat.reply" => {
            let args: ReplyArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("text", error))?;
            let text = message_text(&args.text).map_err(|error| text_error("text", error))?;
            validate_suggestions(&args.suggestions).map_err(|error| {
                ToolError::invalid("invalid_arguments", "suggestions", error.to_string())
            })?;
            Ok(ToolCall::Reply {
                text,
                suggestions: args.suggestions,
            })
        }
        "chat.ask_user" => {
            let args: AskArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("question", error))?;
            let question =
                message_text(&args.question).map_err(|error| text_error("question", error))?;
            Ok(ToolCall::AskUser(question))
        }
        "chat.progress" => {
            let args: ProgressArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("text", error))?;
            Ok(ToolCall::Progress(progress_text(&args.text)?))
        }
        "project.create" => {
            let args: ProjectCreateArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("name", error))?;
            let name = name_field("name", &args.name)?;
            let path = args
                .path
                .map(|value| name_field("path", &value))
                .transpose()?;
            Ok(ToolCall::ProjectCreate(ProjectCreate { name, path }))
        }
        "agent.spawn" => {
            let args: AgentSpawnArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("name", error))?;
            let name = name_field("name", &args.name)?;
            let role = args.role.unwrap_or_else(|| "specialist".to_string());
            if !matches!(role.as_str(), "lead" | "specialist") {
                return Err(ToolError::invalid(
                    "invalid_arguments",
                    "role",
                    "role must be lead or specialist",
                ));
            }
            let project_id = args
                .project_id
                .map(|value| name_field("project_id", &value))
                .transpose()?;
            Ok(ToolCall::AgentSpawn(AgentSpawn {
                name,
                role,
                project_id,
            }))
        }
        "delegation.assign" => {
            let args: DelegationAssignArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("to_agent_id", error))?;
            let to_agent_id = name_field("to_agent_id", &args.to_agent_id)?;
            let task = message_text(&args.task).map_err(|error| text_error("task", error))?;
            let conversation_id = args
                .conversation_id
                .map(|value| name_field("conversation_id", &value))
                .transpose()?;
            let steps = args.steps.unwrap_or(1);
            if steps == 0 {
                return Err(ToolError::invalid("invalid_arguments", "steps", "steps is at least 1"));
            }
            let plan_id = args
                .plan_id
                .map(|value| name_field("plan_id", &value))
                .transpose()?;
            Ok(ToolCall::DelegationAssign(DelegationAssign {
                to_agent_id,
                task,
                conversation_id,
                steps,
                plan_id,
            }))
        }
        "plan.propose" => plan::propose(request),
        "plan.status" => plan::status(request),
        "delegation.complete" => {
            let args: DelegationCompleteArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("delegation_id", error))?;
            let delegation_id = name_field("delegation_id", &args.delegation_id)?;
            let result = message_text(&args.result).map_err(|error| text_error("result", error))?;
            Ok(ToolCall::DelegationComplete(DelegationComplete {
                delegation_id,
                result,
            }))
        }
        "approval.request" => approval::parse(request),
        "approval.decide" => decide::parse(request),
        "approval.pending" => pending::parse(request),
        "status.overview" => status::parse(request),
        "mail.accounts" | "mail.draft_create" | "mail.draft_update" | "mail.draft_get"
        | "mail.attach_file" | "mail.inbox_list" | "mail.message_get" | "mail.triage"
        | "mail.folders" | "mail.move_propose" | "mail.move_apply" | "mail.undo"
        | "mail.undo_list" => Ok(ToolCall::Mail(mail::parse(request)?)),
        "note.save" => Ok(ToolCall::Note(note::save(request)?)),
        "note.search" => Ok(ToolCall::Note(note::search(request)?)),
        "note.update" => Ok(ToolCall::Note(note::update(request)?)),
        "note.delete" => Ok(ToolCall::Note(note::delete(request)?)),
        "contact.find" => Ok(ToolCall::Contact(contact::find(request)?)),
        "contact.save" => Ok(ToolCall::Contact(contact::save(request)?)),
        "contact.delete" => Ok(ToolCall::Contact(contact::delete(request)?)),
        "calendar.timezone" => Ok(ToolCall::Calendar(calendar::timezone(request)?)),
        "calendar.list" => Ok(ToolCall::Calendar(calendar::list(request)?)),
        "calendar.create" => Ok(ToolCall::Calendar(calendar::create(request)?)),
        "calendar.update" => Ok(ToolCall::Calendar(calendar::update(request)?)),
        "calendar.delete" => Ok(ToolCall::Calendar(calendar::delete(request)?)),
        "alert.rule_create" => Ok(ToolCall::Alert(alert::rule_create(request)?)),
        "alert.rule_list" => Ok(ToolCall::Alert(alert::rule_list(request)?)),
        "alert.rule_update" => Ok(ToolCall::Alert(alert::rule_update(request)?)),
        "alert.rule_delete" => Ok(ToolCall::Alert(alert::rule_delete(request)?)),
        "alert.mode_set" => Ok(ToolCall::Alert(alert::mode_set(request)?)),
        other => Err(ToolError::invalid(
            "unknown_tool",
            "tool",
            format!(
                "unknown MCP tool {other:?}; use chat.reply, chat.ask_user, chat.progress, \
                 project.create, agent.spawn, delegation.assign, delegation.complete, \n                 plan.propose, plan.status, \
                 approval.request, approval.decide, approval.pending, status.overview, \
                 a mail.*, note.*, contact.*, calendar.* or alert.* tool"
            ),
        )),
    }
}

/// One plain line of 1 to 120 characters. Control characters are refused, runs
/// of spaces collapse, and text outside the bounds is refused, never cut.
pub(super) fn progress_text(raw: &str) -> Result<String, ToolError> {
    if raw.chars().any(char::is_control) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "text",
            "text cannot contain control characters",
        ));
    }
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if !(1..=PROGRESS_MAX_CHARS).contains(&text.chars().count()) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "text",
            format!("text must be 1 to {PROGRESS_MAX_CHARS} characters"),
        ));
    }
    Ok(text)
}

pub(super) fn name_field(field: &'static str, raw: &str) -> Result<String, ToolError> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            field,
            format!("{field} must be 1..=200 characters without controls"),
        ));
    }
    Ok(value.to_string())
}

/// A tool that takes no arguments: a field it does not know is refused, never
/// ignored, so a caller cannot ask for a wider view through it.
pub(super) fn no_arguments(request: &ToolRequest) -> Result<(), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NoArguments {}

    serde_json::from_value::<NoArguments>(request.arguments.clone())
        .map(drop)
        .map_err(|error| invalid_arguments("arguments", error))
}

/// One plain line of at most `max` characters, trimmed. Control characters are
/// refused and text past the bound is refused, never cut.
pub(super) fn one_line(field: &'static str, raw: &str, max: usize) -> Result<String, ToolError> {
    let value = raw.trim();
    if value.chars().any(char::is_control) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            field,
            format!("{field} cannot contain control characters"),
        ));
    }
    if value.chars().count() > max {
        return Err(ToolError::invalid(
            "invalid_arguments",
            field,
            format!("{field} is at most {max} characters"),
        ));
    }
    Ok(value.to_string())
}

pub(super) fn arguments<T: DeserializeOwned>(
    request: &ToolRequest,
    field: &'static str,
) -> Result<T, ToolError> {
    serde_json::from_value(request.arguments.clone())
        .map_err(|error| invalid_arguments(field, error))
}

pub(super) fn invalid_arguments(field: &'static str, error: serde_json::Error) -> ToolError {
    ToolError::invalid(
        "invalid_arguments",
        field,
        format!("expected a string field {field:?}: {error}"),
    )
}

pub(super) fn text_error(field: &'static str, error: TextError) -> ToolError {
    let code = match error {
        TextError::Empty => "empty_message",
        TextError::ControlCharacters => "control_characters",
        TextError::TooLong => "invalid_body",
    };
    ToolError::invalid(code, field, error.to_string())
}
