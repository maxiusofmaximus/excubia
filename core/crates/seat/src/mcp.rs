//! `hn-mcp`: a tiny MCP stdio server, one process per run.
//!
//! It exposes only `chat.reply` and `chat.ask_user`, validates their arguments
//! and forwards them to the host over a loopback HTTP listener with the
//! per-run token. It has no filesystem, shell or network tools of its own.

use std::time::Duration;

use hn_core_chat::{ToolError, ToolErrorKind, ToolReply, ToolRequest};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::http::{self, LoopbackUrl};
use crate::tools;

const PROTOCOL_VERSION: &str = "2024-11-05";
/// The longest a tool call may take. The slow ones are the panel spawns: the
/// UAC canary runs before the panel exists and may install its provider
/// package in the shared cache on the first turn of a host.
const CALL_TIMEOUT: Duration = Duration::from_secs(75);

pub async fn run_stdio() -> Result<(), String> {
    let url = std::env::var("HN_TOOL_URL")
        .map_err(|_| "HN_TOOL_URL is not set; hn-mcp only runs under the seat".to_string())?;
    let token = std::env::var("HN_TOOL_TOKEN")
        .map_err(|_| "HN_TOOL_TOKEN is not set; hn-mcp only runs under the seat".to_string())?;
    if token.contains(['\r', '\n']) {
        return Err("HN_TOOL_TOKEN contains a newline".to_string());
    }

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                write_message(
                    &mut stdout,
                    &rpc_error(Value::Null, -32700, &format!("parse error: {error}")),
                )
                .await?;
                continue;
            }
        };
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let response = match method {
            "initialize" => id.map(|id| {
                rpc_result(
                    id,
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": {
                            "name": "hn-core-chat",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                )
            }),
            "notifications/initialized" | "notifications/cancelled" => None,
            "ping" => id.map(|id| rpc_result(id, json!({}))),
            "tools/list" => id.map(|id| rpc_result(id, tools::definitions())),
            "tools/call" => match id {
                Some(id) => {
                    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
                    Some(call_tool(id, &params, &url, &token).await)
                }
                None => None,
            },
            other => id.map(|id| rpc_error(id, -32601, &format!("method not found: {other}"))),
        };
        if let Some(response) = response {
            write_message(&mut stdout, &response).await?;
        }
    }
    Ok(())
}

async fn call_tool(id: Value, params: &Value, url: &str, token: &str) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return rpc_error(id, -32602, "tools/call requires params.name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let request = ToolRequest {
        tool: name.to_string(),
        arguments,
    };
    match tools::parse(&request) {
        Ok(_) => match post_tool(url, token, &request).await {
            Ok(reply) => tool_content(id, reply.result, false),
            Err(error) => tool_content(id, tool_error_value(&error), true),
        },
        Err(error) => tool_content(id, tool_error_value(&error), true),
    }
}

fn tool_content(id: Value, value: Value, is_error: bool) -> Value {
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    rpc_result(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "isError": is_error
        }),
    )
}

fn tool_error_value(error: &ToolError) -> Value {
    json!({
        "ok": false,
        "code": error.code,
        "field": error.field,
        "message": error.message
    })
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

async fn write_message(stdout: &mut tokio::io::Stdout, value: &Value) -> Result<(), String> {
    let mut line = serde_json::to_string(value).map_err(|error| error.to_string())?;
    line.push('\n');
    stdout
        .write_all(line.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stdout.flush().await.map_err(|error| error.to_string())
}

async fn post_tool(url: &str, token: &str, request: &ToolRequest) -> Result<ToolReply, ToolError> {
    let endpoint = LoopbackUrl::parse(url).map_err(|error| transport_error(&error))?;
    let body = serde_json::to_vec(&json!({
        "tool": request.tool,
        "arguments": request.arguments
    }))
    .map_err(|error| transport_error(&format!("could not encode tool call: {error}")))?;
    let response = http::request(
        &endpoint,
        "POST",
        &[
            ("Content-Type", "application/json"),
            ("X-HN-Tool-Token", token),
        ],
        &body,
        CALL_TIMEOUT,
    )
    .await
    .map_err(|error| transport_error(&error))?;
    parse_http(&response.status, &response.body)
}

fn parse_http(status: &u16, body: &[u8]) -> Result<ToolReply, ToolError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| transport_error("the host answered with invalid JSON"))?;
    if (200..300).contains(status) && value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(ToolReply {
            result: value.get("result").cloned().unwrap_or_else(|| json!({})),
        });
    }
    let detail = value.get("detail").cloned().unwrap_or_else(|| json!({}));
    let code = static_code(detail.get("code").and_then(Value::as_str));
    let field = static_field(detail.get("field").and_then(Value::as_str));
    let message = detail
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the host rejected the tool call")
        .to_string();
    let kind = match status {
        401 => ToolErrorKind::Unauthorized,
        403 => ToolErrorKind::Forbidden,
        400 | 422 => ToolErrorKind::Invalid,
        409 => ToolErrorKind::Conflict,
        _ => ToolErrorKind::Unavailable,
    };
    Err(ToolError {
        kind,
        code,
        field,
        message,
    })
}

fn static_code(code: Option<&str>) -> &'static str {
    match code {
        Some("invalid_tool_token") => "invalid_tool_token",
        Some("invalid_body") => "invalid_body",
        Some("invalid_arguments") => "invalid_arguments",
        Some("empty_message") => "empty_message",
        Some("control_characters") => "control_characters",
        Some("unknown_tool") => "unknown_tool",
        Some("already_replied") => "already_replied",
        Some("run_closed") => "run_closed",
        Some("scope_denied") => "scope_denied",
        Some("depth_exceeded") => "depth_exceeded",
        Some("cycle_detected") => "cycle_detected",
        Some("unknown_project") => "unknown_project",
        Some("unknown_agent") => "unknown_agent",
        Some("invalid_path") => "invalid_path",
        Some("seat_limit") => "seat_limit",
        Some("pane_limit") => "pane_limit",
        Some("project_exists") => "project_exists",
        Some("lead_exists") => "lead_exists",
        Some("agent_busy") => "agent_busy",
        Some("seat_unavailable") => "seat_unavailable",
        Some("no_such_delegation") => "no_such_delegation",
        Some("delegation_cancelled") => "delegation_cancelled",
        Some("not_your_delegation") => "not_your_delegation",
        Some("turn_budget") => "turn_budget",
        Some("hour_budget") => "hour_budget",
        Some("no_such_approval") => "no_such_approval",
        Some("approval_scope_denied") => "approval_scope_denied",
        Some("approval_expired") => "approval_expired",
        Some("approval_already_decided") => "approval_already_decided",
        Some("approval_level_mismatch") => "approval_level_mismatch",
        Some("approval_not_delegable") => "approval_not_delegable",
        Some("approval_self_approval") => "approval_self_approval",
        Some("approval_budget") => "approval_budget",
        Some("chat_unavailable") => "chat_unavailable",
        _ => "tool_error",
    }
}

fn static_field(field: Option<&str>) -> Option<&'static str> {
    match field {
        Some("text") => Some("text"),
        Some("question") => Some("question"),
        Some("tool") => Some("tool"),
        Some("name") => Some("name"),
        Some("path") => Some("path"),
        Some("role") => Some("role"),
        Some("project_id") => Some("project_id"),
        Some("to_agent_id") => Some("to_agent_id"),
        Some("task") => Some("task"),
        Some("result") => Some("result"),
        Some("delegation_id") => Some("delegation_id"),
        Some("conversation_id") => Some("conversation_id"),
        Some("approval_id") => Some("approval_id"),
        Some("decision") => Some("decision"),
        _ => None,
    }
}

fn transport_error(message: &str) -> ToolError {
    ToolError {
        kind: ToolErrorKind::Unavailable,
        code: "tool_unreachable",
        field: None,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[test]
    fn endpoint_parser_rejects_non_http() {
        assert!(LoopbackUrl::parse("https://127.0.0.1:1/x").is_err());
        let endpoint =
            LoopbackUrl::parse("http://127.0.0.1:4242/internal/chat/tools").expect("url");
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 4242);
        assert_eq!(endpoint.path, "/internal/chat/tools");
    }

    #[test]
    fn http_errors_keep_the_contract_detail() {
        let body = br#"{"detail":{"code":"invalid_body","field":"text","message":"too long"}}"#;
        let error = parse_http(&422, body).expect_err("error");
        assert_eq!(error.kind, ToolErrorKind::Invalid);
        assert_eq!(error.code, "invalid_body");
        assert_eq!(error.field, Some("text"));
    }

    #[tokio::test]
    async fn post_tool_maps_a_mock_422_to_an_invalid_tool_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock");
        let address = listener.local_addr().expect("mock address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0_u8; 2048];
            let _ = socket.read(&mut buffer).await;
            let body = br#"{"detail":{"code":"invalid_body","field":"text","message":"too long"}}"#;
            let head = format!(
                "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.expect("mock head");
            socket.write_all(body).await.expect("mock body");
        });
        let request = ToolRequest {
            tool: "chat.reply".to_string(),
            arguments: json!({ "text": "x" }),
        };
        let error = post_tool(
            &format!("http://{address}/internal/chat/tools"),
            "test-token",
            &request,
        )
        .await
        .expect_err("422 is an error");
        assert_eq!(error.kind, ToolErrorKind::Invalid);
        assert_eq!(error.code, "invalid_body");
        assert_eq!(error.field, Some("text"));
        server.await.expect("mock task");
    }

    #[test]
    fn http_success_returns_the_result() {
        let body = br#"{"ok":true,"result":{"status":"completed"}}"#;
        let reply = parse_http(&200, body).expect("reply");
        assert_eq!(reply.result["status"], "completed");
    }
}
