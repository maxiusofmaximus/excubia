//! Internal MCP tool endpoint. It is never mounted on the public listener:
//! the host binds a second, loopback-only listener, and the request also needs
//! the per-run token the seat handed to the CLI process.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_chat::{ToolError, ToolErrorKind, ToolRequest};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::parse_body;
use crate::ApiContext;

pub const TOOL_TOKEN_HEADER: &str = "x-hn-tool-token";

#[derive(Deserialize)]
struct ToolBody {
    tool: String,
    arguments: Value,
}

pub async fn call(State(ctx): State<ApiContext>, headers: HeaderMap, body: Bytes) -> Response {
    let token = match headers
        .get(TOOL_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|token| !token.is_empty())
    {
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
