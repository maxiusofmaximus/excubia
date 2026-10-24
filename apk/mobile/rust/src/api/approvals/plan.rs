//! `/v1/chat/approvals` request plans. Route, query and body are built and
//! validated here; Dart only signs and sends the bytes.

use serde_json::{json, Map, Value};

use super::model::{MODES, STATUSES};
use crate::api::chat::plan::ChatPlan;
use crate::api::error::{ErrorInfo, FailureKind};
use crate::api::validate;
use crate::api::write::HttpCall;

/// The inbox of the operator: pending by default, any lifecycle state on request.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_inbox(conversation_id: Option<String>, status: Option<String>) -> ChatPlan {
    let mut query = Vec::new();
    if let Some(status) = status {
        if !STATUSES.contains(&status.as_str()) {
            return problem();
        }
        query.push(format!("status={status}"));
    }
    if let Some(conversation_id) = conversation_id {
        if !validate::identifier(&conversation_id) {
            return problem();
        }
        query.push(format!("conversation_id={conversation_id}"));
    }
    let target = if query.is_empty() {
        "/v1/chat/approvals".to_string()
    } else {
        format!("/v1/chat/approvals?{}", query.join("&"))
    };
    get(target)
}

/// The state and the deadline of one approval, never its payload.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_read(approval_id: String) -> ChatPlan {
    if !validate::identifier(&approval_id) {
        return problem();
    }
    get(format!("/v1/chat/approvals/{approval_id}"))
}

/// One answer to one approval. `client_id` makes a retry safe: the host gives
/// the same answer to the same id.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_decision(approval_id: String, approve: bool, client_id: String) -> ChatPlan {
    if !validate::identifier(&approval_id) || !validate::client_id(&client_id) {
        return problem();
    }
    let decision = if approve { "approve" } else { "deny" };
    post(
        format!("/v1/chat/approvals/{approval_id}/decision"),
        object(json!({
            "decision": decision,
            "scope": "once",
            "client_id": client_id,
        })),
    )
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_settings_read() -> ChatPlan {
    get("/v1/chat/approvals/settings".to_string())
}

/// Set the global mode (`project_id` absent) or one project's override.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_settings_write(
    mode: String,
    project_id: Option<String>,
    client_id: String,
) -> ChatPlan {
    if !MODES.contains(&mode.as_str()) || !validate::client_id(&client_id) {
        return problem();
    }
    let mut body = Map::new();
    body.insert("client_id".to_string(), Value::String(client_id));
    body.insert("mode".to_string(), Value::String(mode));
    if let Some(project_id) = project_id {
        if !validate::identifier(&project_id) {
            return problem();
        }
        body.insert("project_id".to_string(), Value::String(project_id));
    }
    post("/v1/chat/approvals/settings".to_string(), body)
}

fn get(target: String) -> ChatPlan {
    ChatPlan {
        request: Some(HttpCall {
            method: "GET".to_string(),
            target,
            body: Vec::new(),
        }),
        problem: None,
    }
}

fn post(target: String, body: Map<String, Value>) -> ChatPlan {
    match serde_json::to_vec(&Value::Object(body)) {
        Ok(bytes) => ChatPlan {
            request: Some(HttpCall {
                method: "POST".to_string(),
                target,
                body: bytes,
            }),
            problem: None,
        },
        Err(_) => problem(),
    }
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn problem() -> ChatPlan {
    ChatPlan {
        request: None,
        problem: Some(ErrorInfo {
            message: "La petición no es válida.".to_string(),
            action: None,
            kind: FailureKind::Retry,
        }),
    }
}
