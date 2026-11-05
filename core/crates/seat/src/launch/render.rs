//! The config files each CLI reads at launch. Pure renderers: the bytes are
//! decided here, the files are written by `LaunchPlan::materialise`.

use std::path::Path;

use serde_json::json;

use super::LaunchError;
use crate::hook_action::CLAUDE_HOOK_MATCHER;

/// Seconds Claude waits for `excubia-uac-hook`: the approval TTL plus a margin, so
/// the hook always answers before Claude gives up on it.
pub(super) const CLAUDE_HOOK_TIMEOUT_SECONDS: u64 = hn_core_chat::DEFAULT_TTL_SECONDS as u64 + 30;

/// The MCP server name the seat declares to every CLI.
pub(super) const MCP_SERVER: &str = "hn-core";

/// Claude names an MCP tool `mcp__<server>__<tool>` with the dots of the tool
/// name turned into `_`.
fn mcp_rule(tool: &str) -> String {
    format!("mcp__{MCP_SERVER}__{}", tool.replace('.', "_"))
}

/// The tools the CLI may run are gated by the blocking hook: the hook opens an
/// approval and exits `2` unless the operator allows it. Claude runs hooks
/// before its permission check, so the hook is the gate, and the permission
/// list must allow the same tools or the approved call is still refused. The
/// hook timeout must outlast the approval TTL (120 s by default), or Claude
/// would treat a timed-out hook as non-blocking and run the command.
///
/// Print mode has no prompt to answer: an MCP tool missing from the list is
/// refused, so the turn ends without `chat.reply` (CORE10 diagnosis).
pub fn claude_settings(hook_command: &Path) -> Result<Vec<u8>, LaunchError> {
    let mut allow: Vec<String> = [
        "Bash",
        "Write",
        "Edit",
        "MultiEdit",
        "NotebookEdit",
        "WebFetch",
    ]
    .map(String::from)
    .to_vec();
    // Every tool the host serves to this seat is meant to be called: the host
    // is the gate (scope, approval, `mail.*` never sends), so print mode must
    // not refuse them. Deriving the rules from the served list keeps the two in
    // step when a tool is added.
    if let Some(tools) = crate::tools::definitions()["tools"].as_array() {
        allow.extend(
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .map(mcp_rule),
        );
    }
    serde_json::to_vec_pretty(&json!({
        "permissions": {
            "allow": allow,
            "deny": ["WebSearch"]
        },
        "hooks": {
            "PreToolUse": [{
                "matcher": CLAUDE_HOOK_MATCHER,
                "hooks": [{
                    "type": "command",
                    "command": format!("\"{}\"", hook_command.to_string_lossy()),
                    "timeout": CLAUDE_HOOK_TIMEOUT_SECONDS
                }]
            }]
        }
    }))
    .map_err(|error| LaunchError::Encode(error.to_string()))
}

pub(super) fn claude_mcp(command: &str, tool_url: &str) -> Result<Vec<u8>, LaunchError> {
    serde_json::to_vec_pretty(&json!({
        "mcpServers": {
            (MCP_SERVER): {
                "type": "stdio",
                "command": command,
                "args": [],
                "env": { "HN_TOOL_URL": tool_url }
            }
        }
    }))
    .map_err(|error| LaunchError::Encode(error.to_string()))
}

pub(super) fn test_mcp(command: &str, tool_url: &str) -> Result<Vec<u8>, LaunchError> {
    serde_json::to_vec_pretty(&json!({
        "mcp": {
            "command": command,
            "args": [],
            "env": { "HN_TOOL_URL": tool_url }
        }
    }))
    .map_err(|error| LaunchError::Encode(error.to_string()))
}

pub(super) fn opencode_config(
    command: &str,
    tool_url: &str,
    role: &str,
) -> Result<Vec<u8>, LaunchError> {
    serde_json::to_vec_pretty(&json!({
        "$schema": "https://opencode.ai/config.json",
        "agent": { "alfred": { "prompt": role } },
        "mcp": {
            "servers": {
                "hn-core": {
                    "type": "local",
                    "command": [command],
                    "environment": { "HN_TOOL_URL": tool_url }
                }
            }
        },
        "permissions": { "shell": "deny", "edit": "deny" }
    }))
    .map_err(|error| LaunchError::Encode(error.to_string()))
}

pub(super) fn codex_config(
    command: &str,
    tool_url: &str,
    role: &str,
) -> Result<Vec<u8>, LaunchError> {
    use toml::Value;

    let mut profile = toml::map::Map::new();
    profile.insert(
        "developer_instructions".to_string(),
        Value::String(role.to_string()),
    );
    let mut profiles = toml::map::Map::new();
    profiles.insert("alfred".to_string(), Value::Table(profile));

    let mut env = toml::map::Map::new();
    env.insert(
        "HN_TOOL_URL".to_string(),
        Value::String(tool_url.to_string()),
    );
    let mut mcp = toml::map::Map::new();
    mcp.insert("command".to_string(), Value::String(command.to_string()));
    mcp.insert("args".to_string(), Value::Array(Vec::new()));
    mcp.insert("env".to_string(), Value::Table(env));
    let mut servers = toml::map::Map::new();
    servers.insert("hn-core".to_string(), Value::Table(mcp));

    let mut root = toml::map::Map::new();
    root.insert("profiles".to_string(), Value::Table(profiles));
    root.insert("mcp_servers".to_string(), Value::Table(servers));
    toml::to_string(&Value::Table(root))
        .map(String::into_bytes)
        .map_err(|error| LaunchError::Encode(error.to_string()))
}
