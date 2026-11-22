//! `/v1/chat/approvals` request plans. Route, query and body are built and
//! validated here; Dart only signs and sends the bytes.

use serde_json::{json, Map, Value};

use super::model::{MODES, STATUSES};
use crate::api::chat::plan::ChatPlan;
use crate::api::error::{ErrorInfo, FailureKind};
use crate::api::validate;
use crate::api::write::HttpCall;

const MAX_EXCLUDE: usize = 20;
const MAX_EXCLUDE_ID_CHARS: usize = 512;

/// The inbox of the operator: pending by default, any lifecycle state on request.
/// A device has no read of one approval by id (`GET /v1/chat/approvals/{id}` is
/// a hook route): it finds one here, by status, as `findApproval` does in Dart.
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

/// One answer to one approval. `client_id` makes a retry safe: the host gives
/// the same answer to the same id. `exclude` leaves mails out of a `mail.move`.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_approval_decision(
    approval_id: String,
    approve: bool,
    client_id: String,
    exclude: Vec<String>,
) -> ChatPlan {
    if !validate::identifier(&approval_id)
        || !validate::client_id(&client_id)
        || !exclusions_allowed(approve, &exclude)
    {
        return problem();
    }
    let decision = if approve { "approve" } else { "deny" };
    let mut body = object(json!({
        "decision": decision,
        "scope": "once",
        "client_id": client_id,
    }));
    if !exclude.is_empty() {
        body.insert("exclude".to_string(), json!(exclude));
    }
    post(format!("/v1/chat/approvals/{approval_id}/decision"), body)
}

/// The host's rules for `exclude` (`api.md`, «Exclusions»): only with `approve`,
/// at most 20 ids, each one non-empty and at most 512 characters.
fn exclusions_allowed(approve: bool, exclude: &[String]) -> bool {
    exclude.is_empty()
        || (approve
            && exclude.len() <= MAX_EXCLUDE
            && exclude
                .iter()
                .all(|id| !id.is_empty() && id.chars().count() <= MAX_EXCLUDE_ID_CHARS))
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
