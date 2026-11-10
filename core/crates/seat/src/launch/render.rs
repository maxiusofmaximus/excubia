//! The config files each CLI reads at launch. Pure renderers: the bytes are
//! decided here, the files are written by `LaunchPlan::materialise`.

use std::path::Path;

use serde_json::json;

use super::LaunchError;
use crate::hook_action::CLAUDE_HOOK_MATCHER;

/// Seconds Claude waits for `excubia-uac-hook`: the approval TTL plus a margin, so
/// the hook always answers before Claude gives up on it.
pub(super) const CLAUDE_HOOK_TIMEOUT_SECONDS: u64 = excubia_core_chat::DEFAULT_TTL_SECONDS as u64 + 30;

/// The MCP server name the seat declares to every CLI.
pub(super) const MCP_SERVER: &str = "excubia-core";

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
                "env": { "EXCUBIA_TOOL_URL": tool_url }
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
            "env": { "EXCUBIA_TOOL_URL": tool_url }
        }
    }))
    .map_err(|error| LaunchError::Encode(error.to_string()))
}

/// The agent body of the preflight canary. The canary runs the same `alfred`
/// agent a seat runs, with a fixed role.
const CANARY_ROLE: &str = "Ejecuta el comando de prueba que te pidan, sin comentarios.";

/// Which run an OpenCode configuration is for. The preflight canary and a seat
/// render through [`opencode_config`], so the canary proves the permission block
/// and the plugin key a seat gets. They differ only in the provider and the MCP
/// server.
pub(crate) enum OpenCodeRun<'a> {
    /// The preflight canary: a local provider answers, no MCP server.
    Canary { provider_url: &'a str },
    /// A seat: the operator's own model, the MCP server and the role.
    Seat {
        command: &'a str,
        tool_url: &'a str,
        role: &'a str,
    },
}

/// The model every Alfred CLI turn runs on: opencode's own default is a free model that runs
/// out of usage. shortcut: one fixed model for all turns, upgrade when the Master picks another.
const SEAT_MODEL: &str = "opencode/crouton-alpha-free";

/// The OpenCode configuration of one run. Edit and fetch are always denied. Shell
/// is allowed only with the UAC plugin: the plugin is then the gate, and a native
/// denial would make an approved command never run (probe on opencode 2.2.6,
/// 2026-11-08, RFC alfred-cli-engine (documento ausente) §8). Without a plugin, shell stays denied.
/// `plugin` is the plugin directory to load, when the run has one.
pub(crate) fn opencode_config(
    plugin: Option<&Path>,
    run: OpenCodeRun<'_>,
) -> Result<Vec<u8>, LaunchError> {
    let (role, provider_url, mcp) = match run {
        OpenCodeRun::Canary { provider_url } => (CANARY_ROLE, Some(provider_url), None),
        OpenCodeRun::Seat {
            command,
            tool_url,
            role,
        } => (role, None, Some((command, tool_url))),
    };
    let shell = if plugin.is_some() { "allow" } else { "deny" };
    let mut body = json!({
        "$schema": "https://opencode.ai/config.json",
        "autoupdate": false,
        "share": "disabled",
        "permission": { "bash": shell, "edit": "deny", "webfetch": "deny" },
        "agent": { "alfred": { "prompt": role } }
    });
    if let Some(plugin) = plugin {
        // The CLI resolves a relative plugin path against its own directory and
        // rejects Windows verbatim paths (`\\?\`), so it gets a plain absolute one.
        let plugin = std::path::absolute(plugin).unwrap_or_else(|_| plugin.to_path_buf());
        body["plugin"] = json!([plugin.to_string_lossy()]);
    }
    if let Some(base_url) = provider_url {
        body["model"] = json!("mock/mock");
        body["provider"] = json!({
            "mock": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Mock",
                "options": { "baseURL": base_url, "apiKey": "dummy" },
                "models": {
                    "mock": { "name": "Mock", "tool_call": true, "limit": { "context": 200000, "output": 8192 } }
                }
            }
        });
    }
    if provider_url.is_none() {
        body["model"] = json!(SEAT_MODEL);
    }
    if let Some((command, tool_url)) = mcp {
        body["mcp"] = json!({
            (MCP_SERVER): {
                "type": "local",
                "command": [command],
                "environment": { "EXCUBIA_TOOL_URL": tool_url }
            }
        });
    }
    serde_json::to_vec_pretty(&body).map_err(|error| LaunchError::Encode(error.to_string()))
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
        "EXCUBIA_TOOL_URL".to_string(),
        Value::String(tool_url.to_string()),
    );
    let mut mcp = toml::map::Map::new();
    mcp.insert("command".to_string(), Value::String(command.to_string()));
    mcp.insert("args".to_string(), Value::Array(Vec::new()));
    mcp.insert("env".to_string(), Value::Table(env));
    let mut servers = toml::map::Map::new();
    servers.insert("excubia-core".to_string(), Value::Table(mcp));

    let mut root = toml::map::Map::new();
    root.insert("profiles".to_string(), Value::Table(profiles));
    root.insert("mcp_servers".to_string(), Value::Table(servers));
    toml::to_string(&Value::Table(root))
        .map(String::into_bytes)
        .map_err(|error| LaunchError::Encode(error.to_string()))
}
