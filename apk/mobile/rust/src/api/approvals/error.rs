//! What a refused approval call means on screen. The host codes come from
//! `errors.md`; the grant line is the canonical command of the design (§9).

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalFailureKind {
    /// `403`: the device has no `APPROVE` grant; `command` is the host line.
    NeedsGrant,
    /// `409 approval_already_decided`: somebody else answered. Re-read the
    /// approval and show its real state without buttons.
    AlreadyDecided,
    /// `409 approval_expired`: the TTL passed without an answer.
    Expired,
    /// Anything else; `code` is the host code when the body names one.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalFailure {
    pub kind: ApprovalFailureKind,
    pub command: Option<String>,
    pub code: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_approval_failure(status: u16, body: String, device_id: String) -> ApprovalFailure {
    let code = refusal_code(&body);
    let kind = match (status, code.as_str()) {
        (403, _) => ApprovalFailureKind::NeedsGrant,
        (409, "approval_already_decided") => ApprovalFailureKind::AlreadyDecided,
        (409, "approval_expired") => ApprovalFailureKind::Expired,
        _ => ApprovalFailureKind::Other,
    };
    let command =
        (kind == ApprovalFailureKind::NeedsGrant).then(|| approval_grant_command(device_id));
    ApprovalFailure {
        kind,
        command,
        code,
    }
}

/// The line that gives this device the `approve` permission on the host.
#[flutter_rust_bridge::frb(sync)]
pub fn approval_grant_command(device_id: String) -> String {
    let device = if device_id.trim().is_empty() {
        "<device_id>".to_string()
    } else {
        device_id.trim().to_string()
    };
    format!("host devices grant {device} --permission approve")
}

/// The host answers `{"detail":"code"}` or `{"detail":{"code":"…"}}`.
fn refusal_code(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return String::new();
    };
    match value.get("detail") {
        Some(Value::String(code)) => code.clone(),
        Some(detail) => detail
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        None => String::new(),
    }
}
