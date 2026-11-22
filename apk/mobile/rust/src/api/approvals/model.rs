//! Approval records as the host sends them: the inbox rows, the decision answer,
//! the single state read and the settings. Dart renders what these return; it
//! never decides a status on its own.

use serde_json::Value;

use crate::api::chat::model::text;

/// The five states of the host lifecycle.
pub(crate) const STATUSES: [&str; 5] = ["pending", "approved", "denied", "expired", "consumed"];
/// The two modes of `/v1/chat/approvals/settings`.
pub(crate) const MODES: [&str; 2] = ["delegated", "always_user"];
/// The destinations of a `mail.move` item (`mail.md`, «Organizar correo»).
const MOVE_DESTS: [&str; 3] = ["junkemail", "deleteditems", "inbox"];

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
    /// The facts of `mail.send`. The host sends `details` only to a device with
    /// `APPROVE`; every other reader gets `None` here and in `mail_attach`.
    pub mail_send: Option<MailSendDetails>,
    /// The facts of `mail.attach_path`, with the same rule as `mail_send`.
    pub mail_attach: Option<MailAttachDetails>,
    /// The batch of `mail.move`, with the same rule as `mail_send`.
    pub mail_move: Option<MailMoveDetails>,
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

/// What the operator reads before approving `mail.send` (contract `mail.md`,
/// «Detalles de la aprobación»). Only the listed fields are read, so a token or a
/// `device_code` can never reach this model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailSendDetails {
    pub account: String,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    /// The first 300 characters of the body, as the host cuts them.
    pub body_preview: String,
    pub attachments: Vec<ApprovalAttachment>,
}

/// What the operator reads before approving `mail.attach_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAttachDetails {
    pub path: String,
    pub size: i64,
    pub sha256: String,
}

/// What the operator reads before approving `mail.move` (contract `mail.md`,
/// «Exclusiones de un lote»). Only items the host could read are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailMoveDetails {
    pub account: String,
    pub items: Vec<MailMoveItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailMoveItem {
    pub message_id: String,
    /// `junkemail`, `deleteditems` or `inbox`.
    pub dest: String,
    pub reason: String,
    pub from_address: Option<String>,
    pub subject: String,
    /// The triage level of the verdict, as the host names it.
    pub level: Option<String>,
    /// Left out by the decision that closed the approval.
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalAttachment {
    pub name: String,
    pub size: i64,
    pub sha256: String,
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
    // A `mail.move` batch also carries an account: its details are never a single send.
    let tool = text(value, "tool");
    let details = value.get("details");
    let batch = tool.as_deref() == Some("mail.move");
    let single = details.filter(|_| !batch);
    let mail_send = single.and_then(mail_send_from);
    let mail_attach = single.and_then(mail_attach_from);
    let mail_move = details.filter(|_| batch).and_then(mail_move_from);
    Some(ChatApproval {
        approval_id,
        conversation_id: text(value, "conversation_id"),
        seat_id: text(value, "seat_id"),
        pane_id: text(value, "pane_id"),
        kind: text(value, "kind"),
        tool,
        summary: text(value, "summary").unwrap_or_default(),
        cwd: text(value, "cwd"),
        risk: text(value, "risk"),
        command: payload.and_then(|payload| text(payload, "command")),
        question: payload.and_then(|payload| text(payload, "question")),
        mail_send,
        mail_attach,
        mail_move,
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

fn mail_send_from(value: &Value) -> Option<MailSendDetails> {
    Some(MailSendDetails {
        account: text(value, "account")?,
        from: text(value, "from"),
        to: texts(value, "to"),
        cc: texts(value, "cc"),
        subject: text(value, "subject").unwrap_or_default(),
        body_preview: text(value, "body_preview").unwrap_or_default(),
        attachments: value
            .get("attachments")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(attachment_from).collect())
            .unwrap_or_default(),
    })
}

fn mail_attach_from(value: &Value) -> Option<MailAttachDetails> {
    Some(MailAttachDetails {
        path: text(value, "path")?,
        size: value.get("size").and_then(Value::as_i64)?,
        sha256: text(value, "sha256")?,
    })
}

/// Without a usable item there is nothing to approve, so the details are absent.
fn mail_move_from(value: &Value) -> Option<MailMoveDetails> {
    let account = text(value, "account")?;
    let items: Vec<MailMoveItem> = value
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(mail_move_item_from)
        .collect();
    (!items.is_empty()).then_some(MailMoveDetails { account, items })
}

fn mail_move_item_from(value: &Value) -> Option<MailMoveItem> {
    let dest = text(value, "dest").filter(|dest| MOVE_DESTS.contains(&dest.as_str()))?;
    Some(MailMoveItem {
        message_id: text(value, "message_id")?,
        dest,
        reason: text(value, "reason").unwrap_or_default(),
        from_address: text(value, "from_address"),
        subject: text(value, "subject").unwrap_or_default(),
        level: text(value, "level"),
        excluded: value
            .get("excluded")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn attachment_from(value: &Value) -> Option<ApprovalAttachment> {
    Some(ApprovalAttachment {
        name: text(value, "name")?,
        size: value.get("size").and_then(Value::as_i64)?,
        sha256: text(value, "sha256")?,
    })
}

fn texts(value: &Value, field: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
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
