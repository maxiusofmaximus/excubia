//! Internal MCP tool endpoint. It is never mounted on the public listener:
//! the host binds a second, loopback-only listener, and the request also needs
//! the per-run token the seat handed to the CLI process.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_core_chat::{ToolError, ToolErrorKind, ToolRequest};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::parse_body;
use crate::ApiContext;

pub const TOOL_TOKEN_HEADER: &str = "x-excubia-tool-token";
/// legacy: the name the MCP shim sent before the rename; read when the current
/// one is absent, never emitted.
const LEGACY_TOOL_TOKEN_HEADER: &str = "x-hn-tool-token";

#[derive(Deserialize)]
struct ToolBody {
    tool: String,
    arguments: Value,
}

pub async fn call(State(ctx): State<ApiContext>, headers: HeaderMap, body: Bytes) -> Response {
    let token = match presented_tool_token(&headers) {
        Some(token) => token.to_string(),
        None => return tool_error(ToolError::unauthorized("missing tool token")),
    };
    let payload: ToolBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let request = ToolRequest {
        tool: payload.tool,
        arguments: payload.arguments,
    };
    match ctx.seat.tool(&token, request).await {
        Ok(reply) => Json(json!({ "ok": true, "result": reply.result })).into_response(),
        Err(error) => tool_error(error),
    }
}

fn tool_error(error: ToolError) -> Response {
    let status = match error.kind {
        ToolErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
        ToolErrorKind::Forbidden => StatusCode::FORBIDDEN,
        ToolErrorKind::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
        ToolErrorKind::Conflict => StatusCode::CONFLICT,
        ToolErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    let mut detail = json!({ "code": error.code, "message": error.message });
    if let Some(field) = error.field {
        detail["field"] = json!(field);
    }
    (status, Json(json!({ "detail": detail }))).into_response()
}

/// The per-run token the MCP shim presents. The pre-rename header is read only when the current one is absent.
fn presented_tool_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(TOOL_TOKEN_HEADER)
        .or_else(|| headers.get(LEGACY_TOOL_TOKEN_HEADER))
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn the_legacy_tool_header_is_read_and_the_current_one_wins() {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_TOOL_TOKEN_HEADER, HeaderValue::from_static("old"));
        assert_eq!(presented_tool_token(&headers), Some("old"));
        headers.insert(TOOL_TOKEN_HEADER, HeaderValue::from_static("new"));
        assert_eq!(presented_tool_token(&headers), Some("new"));
    }
}
