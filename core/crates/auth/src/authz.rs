//! Per-device authorization: which endpoint a device may call, per the matrix
//! in `docs/contracts/auth.md`.
//!
//! The matrix is load-bearing: an endpoint added without a rule fails closed
//! (a `403`, never an implicit allowance). The query string is not part of the
//! match; the path is the one that was signed.

use crate::error::{AuthzReason, Forbidden};
use crate::store::Permission;

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
        (["", "v1", "devices", _id, "revoke"], "POST") => Some("revoke"),
        _ => None,
    };
    if let Some(rule) = chat_rule(method.as_str(), &segments) {
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
        ("GET", "/v1/devices") => Ok(rule(Permission::Admin, Budget::Read)),
        // Starting a conversation launches an agent on the host.
        ("POST", "/v1/workspaces") => Ok(rule(Permission::Respond, Budget::Write)),
        _ if device_action == Some("revoke") => Ok(rule(Permission::Admin, Budget::Write)),
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

/// The `/v1/chat` matrix (`orchestrator-chat.md` §3): reads are `READ_STATE`,
/// mutations are `RESPOND`, and the read cursor is as cheap as `seen`.
fn chat_rule(method: &str, segments: &[&str]) -> Option<EndpointRule> {
    let rule = match (segments, method) {
        (["", "v1", "chat", "bootstrap"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "conversations"], "GET") => rule(Permission::ReadState, Budget::Read),
        (["", "v1", "chat", "conversations"], "POST") => rule(Permission::Respond, Budget::Write),
        (["", "v1", "chat", "conversations", _id], "GET") => {
            rule(Permission::ReadState, Budget::Read)
        }
        (["", "v1", "chat", "conversations", _id, "update"], "POST") => {
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
mod tests {
    use super::*;

    #[test]
    fn the_matrix_classifies_every_route() {
        assert_eq!(
            classify("GET", "/v1/state").unwrap().permission,
            Some(Permission::ReadState)
        );
        assert_eq!(
            classify("GET", "/v1/state?x=1").unwrap().budget,
            Budget::Read,
            "the query string is not part of the match"
        );
        assert_eq!(
            classify("POST", "/v1/agents/w1:p1/respond")
                .unwrap()
                .permission,
            Some(Permission::Respond)
        );
        assert_eq!(
            classify("POST", "/v1/agents/w1:p1/keys")
                .unwrap()
                .permission,
            Some(Permission::SendKey)
        );
        assert_eq!(
            classify("POST", "/v1/agents/w1:p1/seen").unwrap().budget,
            Budget::None
        );
        assert_eq!(
            classify("GET", "/v1/devices").unwrap().permission,
            Some(Permission::Admin)
        );
        assert_eq!(
            classify("POST", "/v1/devices/dev-1/revoke").unwrap().budget,
            Budget::Write
        );
        assert_eq!(
            classify("POST", "/v1/workspaces").unwrap().permission,
            Some(Permission::Respond)
        );
        assert_eq!(
            classify("GET", "/v1/catalog").unwrap().permission,
            Some(Permission::ReadState)
        );
        assert_eq!(
            classify("POST", "/v1/agents/w1:p1/stop")
                .unwrap()
                .permission,
            Some(Permission::Respond)
        );
        assert!(classify("DELETE", "/v1/state").is_err(), "fails closed");
        assert!(classify("POST", "/v1/nope").is_err(), "no rule, no entry");
    }

    #[test]
    fn the_chat_matrix_separates_reads_from_writes() {
        let reads = [
            ("GET", "/v1/chat/bootstrap"),
            ("GET", "/v1/chat/conversations"),
            ("GET", "/v1/chat/conversations/cnv_1"),
            ("GET", "/v1/chat/conversations/cnv_1/timeline"),
            ("GET", "/v1/chat/conversations/cnv_1/messages"),
            ("GET", "/v1/chat/sections"),
        ];
        for (method, path) in reads {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(Permission::ReadState), "{path}");
            assert_eq!(rule.budget, Budget::Read, "{path}");
        }
        let writes = [
            ("POST", "/v1/chat/conversations"),
            ("POST", "/v1/chat/conversations/cnv_1/update"),
            ("POST", "/v1/chat/conversations/cnv_1/messages"),
            ("POST", "/v1/chat/sections"),
        ];
        for (method, path) in writes {
            let rule = classify(method, path).expect(path);
            assert_eq!(rule.permission, Some(Permission::Respond), "{path}");
            assert_eq!(rule.budget, Budget::Write, "{path}");
        }
        let read = classify("POST", "/v1/chat/conversations/cnv_1/read").expect("read");
        assert_eq!(read.permission, Some(Permission::ReadState));
        assert_eq!(read.budget, Budget::None);
        assert!(classify("DELETE", "/v1/chat/conversations/cnv_1").is_err());
        assert!(classify("POST", "/v1/chat/bootstrap").is_err());
    }

    /// The approval surface: the inbox is a read and the decision spends the
    /// `APPROVE` grant, which nothing else does.
    #[test]
    fn the_approval_matrix_separates_the_inbox_from_the_decision() {
        let inbox = classify("GET", "/v1/chat/approvals").expect("inbox");
        assert_eq!(inbox.permission, Some(Permission::ReadState));
        assert_eq!(inbox.budget, Budget::Read);
        let decision = classify("POST", "/v1/chat/approvals/apr_1/decision").expect("decision");
        assert_eq!(decision.permission, Some(Permission::Approve));
        assert_eq!(decision.budget, Budget::Write);
        // The hook routes are not device routes: they carry no rule and fail
        // closed for a signed client.
        assert!(classify("POST", "/v1/chat/approvals").is_err());
        assert!(classify("GET", "/v1/chat/approvals/apr_1").is_err());
        assert!(classify("POST", "/v1/chat/approvals/apr_1/consume").is_err());
    }

    #[test]
    fn changing_the_approval_mode_needs_approve_not_admin() {
        let read = classify("GET", "/v1/chat/approvals/settings").expect("settings");
        assert_eq!(read.permission, Some(Permission::ReadState));
        let change = classify("POST", "/v1/chat/approvals/settings").expect("change");
        assert_eq!(change.permission, Some(Permission::Approve));
        assert_eq!(change.budget, Budget::Write);
        let admin_only = [Permission::Admin];
        assert!(authorize(&admin_only, &change).is_err());
    }

    #[test]
    fn a_grant_is_required_for_protected_routes() {
        let respond = classify("POST", "/v1/agents/w1:p1/respond").unwrap();
        assert!(authorize(&[Permission::ReadState], &respond).is_err());
        assert!(authorize(&[Permission::ReadState, Permission::Respond], &respond).is_ok());
        let health = classify("GET", "/v1/health").unwrap();
        assert!(authorize(&[], &health).is_ok());
    }
}
