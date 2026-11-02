//! Per-device authorization: which endpoint a device may call, per the matrix
//! in `docs/contracts/auth.md`.
//!
//! The matrix is load-bearing: an endpoint added without a rule fails closed
//! (a `403`, never an implicit allowance). The query string is not part of the
//! match; the path is the one that was signed.

use crate::error::{AuthzReason, Forbidden};
use crate::store::Permission;

mod agenda;
mod hires;
use agenda::agenda_rule;
use hires::hire_rule;

/// How an endpoint is priced against the rate limiters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    /// No budget (health, pairing, and `seen`).
    None,
    /// The read budget, consumed by every authenticated request.
    Read,
    /// The read budget plus the write budget.
    Write,
}

#[derive(Clone, Copy, Debug)]
pub struct EndpointRule {
    pub permission: Option<Permission>,
    pub budget: Budget,
}

fn rule(permission: Permission, budget: Budget) -> EndpointRule {
    EndpointRule {
        permission: Some(permission),
        budget,
    }
}

/// The permission an endpoint requires, or `None` when it needs none.
pub fn classify(method: &str, target: &str) -> Result<EndpointRule, Forbidden> {
    let path = target.split('?').next().unwrap_or(target);
    let method = method.to_uppercase();
    let unknown = Forbidden {
        reason: AuthzReason::UnknownEndpoint,
        required: None,
    };
    let segments: Vec<&str> = path.split('/').collect();
    let agent_action = match (segments.as_slice(), method.as_str()) {
        (["", "v1", "agents", _pane, action], "POST") => Some(*action),
        _ => None,
    };
    let device_action = match (segments.as_slice(), method.as_str()) {
        (["", "v1", "devices", _id, action], "POST") => Some(*action),
        _ => None,
    };
    if let Some(rule) = chat_rule(method.as_str(), &segments) {
        return Ok(rule);
    }
    if let Some(rule) = mail_send_rule(method.as_str(), &segments) {
        return Ok(rule);
    }
    if let Some(rule) = hire_rule(method.as_str(), &segments) {
        return Ok(rule);
    }
    if let Some(rule) = agenda_rule(method.as_str(), &segments) {
        return Ok(rule);
    }

    match (method.as_str(), path) {
        // Public by design; listed so the matrix is auditable in one place.
        ("GET", "/v1/health") => Ok(EndpointRule {
            permission: None,
            budget: Budget::None,
        }),
        // The one unauthenticated write: the code (or the owner check) is the
        // authorisation, and the endpoint never runs through the verifier.
        ("POST", "/v1/pair") => Ok(EndpointRule {
            permission: None,
            budget: Budget::None,
        }),
        ("GET", "/v1/state") => Ok(rule(Permission::ReadState, Budget::Read)),
        ("GET", "/v1/events") => Ok(rule(Permission::ReadState, Budget::Read)),
        ("GET", "/v1/catalog") => Ok(rule(Permission::ReadState, Budget::Read)),
        ("GET", "/v1/roles") => Ok(rule(Permission::ReadState, Budget::Read)),
        ("GET", "/v1/devices") => Ok(rule(Permission::Admin, Budget::Read)),
        // The mail logins create accounts on the host: an admin surface, and
        // the listing names the accounts, so it is not for ReadState devices.
        ("POST", "/v1/mail/login/start") => Ok(rule(Permission::Admin, Budget::Write)),
        ("GET", "/v1/mail/login/status") => Ok(rule(Permission::Admin, Budget::Read)),
        ("GET", "/v1/mail/accounts") => Ok(rule(Permission::Admin, Budget::Read)),
        // Starting a conversation launches an agent on the host.
        ("POST", "/v1/workspaces") => Ok(rule(Permission::Respond, Budget::Write)),
        // A voice note is a message in other form: the same permission, the same budget.
        ("POST", "/v1/voice/transcribe") => Ok(rule(Permission::Respond, Budget::Write)),
        // A device names its own push endpoint: a write to its own record only.
        ("POST", "/v1/me/push-endpoint") => Ok(rule(Permission::ReadState, Budget::Write)),
        _ if matches!(device_action, Some("revoke" | "grant")) => {
            Ok(rule(Permission::Admin, Budget::Write))
        }
        _ => match agent_action {
            Some("respond") => Ok(rule(Permission::Respond, Budget::Write)),
            Some("stop") => Ok(rule(Permission::Respond, Budget::Write)),
            Some("keys") => Ok(rule(Permission::SendKey, Budget::Write)),
            Some("read") => Ok(rule(Permission::ReadState, Budget::Write)),
            // Marking an agent seen is non-destructive and reversible; it
            // consumes no write budget.
            Some("seen") => Ok(rule(Permission::ReadState, Budget::None)),
            _ => Err(unknown),
        },
    }
}

/// The mail send matrix (MAIL2A). Sending, cancelling and reading a send are the
/// operator's `APPROVE` grant, which no agent holds. Granting or revoking the
/// send consent is `ADMIN`, like the login that creates the account.
fn mail_send_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "mail", "drafts", _id], "GET") => rule(Permission::Approve, Budget::Read),
        (["", "v1", "mail", "drafts", _id, "send"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "mail", "drafts", _id, "update"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "mail", "sends", _id, "cancel"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "mail", "sends", _id], "GET") => rule(Permission::Approve, Budget::Read),
        (["", "v1", "mail", "send", "enable"], "POST") => rule(Permission::Admin, Budget::Write),
        (["", "v1", "mail", "send", "enable", "status"], "GET") => {
            rule(Permission::Admin, Budget::Read)
        }
        (["", "v1", "mail", "send", "disable"], "POST") => rule(Permission::Admin, Budget::Write),
        _ => return None,
    };
    Some(rule)
}

/// The `/v1/chat` matrix (`orchestrator-chat.md` §3): reads are `READ_STATE`,
/// mutations are `RESPOND`, and the read cursor is as cheap as `seen`.
fn chat_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "chat", "bootstrap"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "conversations"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "conversations"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "chat", "conversations", "counts"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "update"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "chat", "conversations", _id], "DELETE") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "chat", "conversations", _id, "timeline"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "messages"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "messages"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        // Marking a conversation read is non-destructive and reversible, as
        // `POST /v1/agents/{pane_id}/seen` is.
        (["", "v1", "chat", "conversations", _id, "read"], "POST") => {
            rule(Permission::ReadState, Budget::None)
        }
        (["", "v1", "chat", "conversations", _id, "cancel"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "chat", "conversations", _id, "stop"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        // Attachments: the upload is the human's `RESPOND`; reading the bytes is `READ_STATE`.
        (["", "v1", "chat", "conversations", _id, "attachments"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "chat", "conversations", _id, "attachments", _att], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "attachments", _att], "DELETE") => {
            rule(Permission::Respond, Budget::Write)
        }
        // A 10 MiB image is 80 chunks: they take the read budget (120/min), not the write one (30/min).
        (["", "v1", "chat", "conversations", _id, "attachments", _att, "chunks"], "POST") => {
            rule(Permission::Respond, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "attachments", _att, "complete"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        (["", "v1", "chat", "conversations", _id, "attachments", _att, "content"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "delegations", _id], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "delegations", _id, "cancel"], "POST") => {
            rule(Permission::Respond, Budget::Write)
        }
        // The UAC inbox is a read; the decision is the only place where the
        // `APPROVE` grant is spent.
        (["", "v1", "chat", "approvals"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "approvals", _id, "decision"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        // The approval mode is the operator's: reading it is `READ_STATE`, and
        // changing it is `APPROVE`, never `ADMIN` (CORE9-D). No agent holds it.
        (["", "v1", "chat", "approvals", "settings"], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "approvals", "settings"], "POST") => {
            rule(Permission::Approve, Budget::Write)
        }
        (["", "v1", "chat", "sections"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "sections"], "POST") => rule(Permission::Respond, Budget::Write),
        _ => return None,
    };
    Some(rule)
}

/// Authorization, or a `Forbidden`. A valid signature never implies
/// authorization: the grant is an explicit list on the device record.
pub fn authorize(permissions: &[Permission], rule: &EndpointRule) -> Result<(), Forbidden> {
    let Some(required) = rule.permission else {
        return Ok(());
    };
    if permissions.contains(&required) {
        return Ok(());
    }
    Err(Forbidden {
        reason: AuthzReason::NotPermitted,
        required: Some(required),
    })
}

#[cfg(test)]
mod tests;
