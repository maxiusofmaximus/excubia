//! The MCP tools CORE6B/CORE7 expose. Arguments are strict JSON objects; text
//! is bounded by the same 32 KiB rule as the HTTP API. An unknown tool is an
//! error, never a shell escape.
//!
//! The approval request and the `tools/list` schemas live in their own
//! modules: they grow with the surface and share nothing with the parser.

mod approval;
mod decide;
mod definitions;
mod pending;

use hn_core_chat::{message_text, TextError, ToolError, ToolRequest};
use serde::Deserialize;

pub use approval::ApprovalRequest;
pub use decide::ApprovalDecide;
pub use definitions::definitions;

#[derive(Clone, Debug, PartialEq)]
pub enum ToolCall {
    Reply(String),
    AskUser(String),
    ProjectCreate(ProjectCreate),
    AgentSpawn(AgentSpawn),
    DelegationAssign(DelegationAssign),
    DelegationComplete(DelegationComplete),
    ApprovalRequest(ApprovalRequest),
    ApprovalDecide(ApprovalDecide),
    ApprovalPending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectCreate {
    pub name: String,
    /// Relative to `workspaces_root`; defaults to a slug of `name`.
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSpawn {
    pub name: String,
    /// `lead` or `specialist`; defaults to `specialist`.
    pub role: String,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationAssign {
    pub to_agent_id: String,
    pub task: String,
    pub conversation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationComplete {
    pub delegation_id: String,
    pub result: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskArgs {
    question: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectCreateArgs {
    name: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentSpawnArgs {
    name: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DelegationAssignArgs {
    to_agent_id: String,
    task: String,
    #[serde(default)]
    conversation_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DelegationCompleteArgs {
    delegation_id: String,
    result: String,
}

pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    match request.tool.as_str() {
        "chat.reply" => {
            let args: ReplyArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("text", error))?;
            let text = message_text(&args.text).map_err(|error| text_error("text", error))?;
            Ok(ToolCall::Reply(text))
        }
        "chat.ask_user" => {
            let args: AskArgs = serde_json::from_value(request.arguments.clone())
                .map_err(|error| invalid_arguments("question", error))?;
            let question =
                message_text(&args.question).map_err(|error| text_error("question", error))?;
            Ok(ToolCall::AskUser(question))
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
            Ok(ToolCall::DelegationAssign(DelegationAssign {
                to_agent_id,
                task,
                conversation_id,
            }))
        }
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
        other => Err(ToolError::invalid(
            "unknown_tool",
            "tool",
            format!(
                "unknown MCP tool {other:?}; use chat.reply, chat.ask_user, project.create, \
                 agent.spawn, delegation.assign, delegation.complete, approval.request, \
                 approval.decide or approval.pending"
            ),
        )),
    }
}

fn name_field(field: &'static str, raw: &str) -> Result<String, ToolError> {
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

fn invalid_arguments(field: &'static str, error: serde_json::Error) -> ToolError {
    ToolError::invalid(
        "invalid_arguments",
        field,
        format!("expected a string field {field:?}: {error}"),
    )
}

fn text_error(field: &'static str, error: TextError) -> ToolError {
    let code = match error {
        TextError::Empty => "empty_message",
        TextError::ControlCharacters => "control_characters",
        TextError::TooLong => "invalid_body",
    };
    ToolError::invalid(code, field, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    #[test]
    fn reply_and_ask_are_typed() {
        assert_eq!(
            parse(&request("chat.reply", json!({"text": "  hola  "}))),
            Ok(ToolCall::Reply("hola".to_string()))
        );
        assert_eq!(
            parse(&request(
                "chat.ask_user",
                json!({"question": "¿proyecto?" })
            )),
            Ok(ToolCall::AskUser("¿proyecto?".to_string()))
        );
    }

    #[test]
    fn team_tools_are_typed() {
        assert_eq!(
            parse(&request(
                "project.create",
                json!({"name": "Demo", "path": "demo"})
            )),
            Ok(ToolCall::ProjectCreate(ProjectCreate {
                name: "Demo".to_string(),
                path: Some("demo".to_string()),
            }))
        );
        assert_eq!(
            parse(&request("agent.spawn", json!({"name": "Bob"}))),
            Ok(ToolCall::AgentSpawn(AgentSpawn {
                name: "Bob".to_string(),
                role: "specialist".to_string(),
                project_id: None,
            }))
        );
        assert!(matches!(
            parse(&request(
                "delegation.assign",
                json!({"to_agent_id": "ag_1", "task": "Plan"})
            )),
            Ok(ToolCall::DelegationAssign(_))
        ));
        assert!(matches!(
            parse(&request(
                "delegation.complete",
                json!({"delegation_id": "dlg_1", "result": "Done"})
            )),
            Ok(ToolCall::DelegationComplete(_))
        ));
    }

    #[test]
    fn invalid_arguments_are_rejected_with_a_field() {
        let missing = parse(&request("chat.reply", json!({}))).expect_err("missing text");
        assert_eq!(missing.code, "invalid_arguments");
        assert_eq!(missing.field, Some("text"));

        let wrong_type = parse(&request("chat.reply", json!({"text": 3}))).expect_err("type");
        assert_eq!(wrong_type.code, "invalid_arguments");

        let empty = parse(&request("chat.reply", json!({"text": "   "}))).expect_err("empty");
        assert_eq!(empty.code, "empty_message");

        let control =
            parse(&request("chat.reply", json!({"text": "a\u{7}"}))).expect_err("control");
        assert_eq!(control.code, "control_characters");

        let too_long = parse(&request(
            "chat.reply",
            json!({"text": "a".repeat(hn_core_chat::MAX_MESSAGE_BYTES + 1)}),
        ))
        .expect_err("too long");
        assert_eq!(too_long.code, "invalid_body");
        assert_eq!(too_long.field, Some("text"));

        let bad_role = parse(&request(
            "agent.spawn",
            json!({"name": "Alfred", "role": "alfred"}),
        ))
        .expect_err("role");
        assert_eq!(bad_role.field, Some("role"));

        let unknown =
            parse(&request("shell.exec", json!({"command": "rm"}))).expect_err("unknown tool");
        assert_eq!(unknown.code, "unknown_tool");
    }

    #[test]
    fn tool_definitions_list_the_team_tools() {
        let names: Vec<String> = definitions()["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| tool["name"].as_str().expect("name").to_string())
            .collect();
        assert_eq!(
            names,
            [
                "chat.reply",
                "chat.ask_user",
                "project.create",
                "agent.spawn",
                "delegation.assign",
                "delegation.complete",
                "approval.request",
                "approval.decide",
                "approval.pending"
            ]
        );
    }

    /// The approval mode is the operator's (CORE9-D, RFC §8): no agent tool
    /// lists it or writes it, and calling one that would is an unknown tool.
    #[test]
    fn no_agent_tool_changes_the_approval_mode() {
        let names = definitions()["tools"].to_string();
        assert!(!names.contains("approval.set_mode"), "{names}");
        assert!(!names.contains("settings"), "{names}");
        for tool in ["approval.set_mode", "approval.settings"] {
            let request = ToolRequest {
                tool: tool.to_string(),
                arguments: json!({ "mode": "always_user" }),
            };
            let refused = parse(&request).expect_err("no such tool");
            assert_eq!(refused.code, "unknown_tool", "{tool}");
        }
    }
}
