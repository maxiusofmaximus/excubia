//! The `approval.request` tool: the agent asks the operator for permission
//! before a risky action. The host computes the digest of the action itself:
//! the agent never chooses it.

use hn_core_chat::{ApprovalKind, ToolError, ToolRequest};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{invalid_arguments, name_field, text_error, ToolCall};

/// The action an agent asks permission for.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRequest {
    pub kind: ApprovalKind,
    pub tool: String,
    pub summary: String,
    pub args: Value,
    pub cwd: Option<String>,
    pub paths: Vec<String>,
    pub ttl_seconds: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalRequestArgs {
    kind: String,
    tool: String,
    summary: String,
    #[serde(default)]
    args: Option<Value>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

/// The arguments of the tool, already bounded.
pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: ApprovalRequestArgs = serde_json::from_value(request.arguments.clone())
        .map_err(|error| invalid_arguments("kind", error))?;
    let kind = match ApprovalKind::from_db(args.kind.trim()) {
        Some(kind) => kind,
        None => {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "kind",
                "kind must be command|network|edit|tool",
            ))
        }
    };
    let tool = name_field("tool", &args.tool)?;
    let summary =
        hn_core_chat::message_text(&args.summary).map_err(|error| text_error("summary", error))?;
    let cwd = match args.cwd {
        Some(cwd) => Some(name_field("cwd", &cwd)?),
        None => None,
    };
    let paths = args
        .paths
        .iter()
        .map(|path| name_field("paths", path))
        .collect::<Result<Vec<_>, _>>()?;
    let ttl_seconds = match args.ttl_seconds {
        Some(seconds) if (1..=hn_core_chat::MAX_TTL_SECONDS as u64).contains(&seconds) => {
            Some(seconds as u32)
        }
        Some(_) => {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "ttl_seconds",
                format!(
                    "ttl_seconds must be between 1 and {}",
                    hn_core_chat::MAX_TTL_SECONDS
                ),
            ))
        }
        None => None,
    };
    Ok(ToolCall::ApprovalRequest(ApprovalRequest {
        kind,
        tool,
        summary,
        args: args.args.unwrap_or_else(|| json!({})),
        cwd,
        paths,
        ttl_seconds,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_chat::ToolRequest;
    use serde_json::json;

    fn request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "approval.request".to_string(),
            arguments,
        }
    }

    /// The approval request is typed like every other tool: the kind is a
    /// closed vocabulary and the arguments are bounded.
    #[test]
    fn the_approval_request_is_typed() {
        let call = parse(&request(
            json!({"kind": "command", "tool": "bash", "summary": "cargo test",
                   "args": {"command": "cargo test"}, "cwd": "/work"}),
        ))
        .expect("approval");
        let ToolCall::ApprovalRequest(approval) = call else {
            panic!("expected an approval request");
        };
        assert_eq!(approval.kind, ApprovalKind::Command);
        assert_eq!(approval.tool, "bash");
        assert!(approval.paths.is_empty());
        assert!(matches!(approval.args, Value::Object(_)));

        let bad_kind = parse(&request(
            json!({"kind": "nope", "tool": "bash", "summary": "x"}),
        ))
        .expect_err("kind");
        assert_eq!(bad_kind.field, Some("kind"));

        let bad_ttl = parse(&request(json!({
            "kind": "command", "tool": "bash", "summary": "x", "ttl_seconds": 9000
        })))
        .expect_err("ttl");
        assert_eq!(bad_ttl.field, Some("ttl_seconds"));
    }
}
