//! Approval records as the host sends them: the inbox rows, the decision answer,
//! the single state read and the settings. Dart renders what these return; it
//! never decides a status on its own.

use serde_json::Value;

use crate::api::chat::model::text;

/// The five states of the host lifecycle.
pub(crate) const STATUSES: [&str; 5] = ["pending", "approved", "denied", "expired", "consumed"];
/// The two modes of `/v1/chat/approvals/settings`.
pub(crate) const MODES: [&str; 2] = ["delegated", "always_user"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalAudit {
    pub device_id: Option<String>,
    pub decision: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatApproval {
    pub approval_id: String,
    pub conversation_id: Option<String>,
    pub seat_id: Option<String>,
    pub pane_id: Option<String>,
    pub kind: Option<String>,
    pub tool: Option<String>,
    pub summary: String,
    pub cwd: Option<String>,
    pub risk: Option<String>,
    /// The redacted command, when the action is a command.
    pub command: Option<String>,
    /// The concrete question, when the host lacked information to decide.
    pub question: Option<String>,
    pub status: String,
    /// `lead`, `cto` or `user`; `None` when the policy decided it.
    pub level: Option<String>,
    pub escalated_from: Option<String>,
    pub decider_kind: Option<String>,
    pub decider_id: Option<String>,
    pub expires_at: Option<String>,
    pub decided_at: Option<String>,
    pub audit: Vec<ApprovalAudit>,
    /// The `revision` of the frame that wrote this view; 0 for a read.
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalProjectMode {
    pub project_id: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSettings {
    pub global: String,
    /// Only the overrides: a project that inherits the global mode is absent.
    pub projects: Vec<ApprovalProjectMode>,
    /// The pending `lead`/`cto` approvals that a switch sent to the operator.
    pub escalated: i64,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_approval_record(json: String) -> Option<ChatApproval> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    // The decision answer nests the record; the state read is flat.
    let record = value.get("approval").unwrap_or(&value);
    approval_from(record)
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_approval_inbox(json: String) -> Option<Vec<ChatApproval>> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let rows = value.get("approvals")?.as_array()?;
    Some(rows.iter().filter_map(approval_from).collect())
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_approval_settings(json: String) -> Option<ApprovalSettings> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let settings = value.get("settings")?;
    let global = mode_of(settings, "global")?;
    let projects = settings
        .get("projects")?
        .as_array()?
        .iter()
        .filter_map(|row| {
            Some(ApprovalProjectMode {
                project_id: text(row, "project_id")?,
                mode: mode_of(row, "mode")?,
            })
        })
        .collect();
    let escalated = value.get("escalated").and_then(Value::as_i64).unwrap_or(0);
    Some(ApprovalSettings {
        global,
        projects,
        escalated,
    })
}

/// Build one view from a record, a row or a frame-shaped object. The host names
/// the id `id` in the inbox and `approval_id` everywhere else.
pub(crate) fn approval_from(value: &Value) -> Option<ChatApproval> {
    let approval_id = text(value, "approval_id").or_else(|| text(value, "id"))?;
    let status = text(value, "status")?;
    if !STATUSES.contains(&status.as_str()) {
        return None;
    }
    let audit = value
        .get("audit")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(audit_from).collect())
        .unwrap_or_default();
    let payload = value.get("payload_redacted");
    Some(ChatApproval {
        approval_id,
        conversation_id: text(value, "conversation_id"),
        seat_id: text(value, "seat_id"),
        pane_id: text(value, "pane_id"),
        kind: text(value, "kind"),
        tool: text(value, "tool"),
        summary: text(value, "summary").unwrap_or_default(),
        cwd: text(value, "cwd"),
        risk: text(value, "risk"),
        command: payload.and_then(|payload| text(payload, "command")),
        question: payload.and_then(|payload| text(payload, "question")),
        status,
        level: text(value, "level"),
        escalated_from: text(value, "escalated_from"),
        decider_kind: text(value, "decider_kind"),
        decider_id: text(value, "decider_id").or_else(|| text(value, "decided_by")),
        expires_at: text(value, "expires_at"),
        decided_at: text(value, "decided_at"),
        audit,
        revision: value.get("revision").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn audit_from(value: &Value) -> Option<ApprovalAudit> {
    Some(ApprovalAudit {
        device_id: text(value, "device_id"),
        decision: text(value, "decision")?,
        decided_at: text(value, "decided_at"),
    })
}

fn mode_of(value: &Value, field: &str) -> Option<String> {
    let mode = text(value, field)?;
    MODES.contains(&mode.as_str()).then_some(mode)
}
