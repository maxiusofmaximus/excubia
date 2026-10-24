//! Approvals the operator answers: the inbox, the decision and the approval
//! mode. Every call is signed like the rest of `/v1/chat`. The host decides what
//! the device may do: a decision and a mode change need `APPROVE`, and a
//! missing grant comes back as `ChatFailure::Forbidden`.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::error::{ChatFailure, ChatResult};
use super::{validate_client_id, validate_prefixed_id};
use crate::client::GatewayClient;

/// The modes the operator can set. `delegated` lets the policy, the scorer and
/// the lead decide first; `always_user` sends every approval to the operator.
pub const APPROVAL_MODES: [&str; 2] = ["delegated", "always_user"];

/// One row of the audit trail of an approval.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ApprovalAudit {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub action_digest: String,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub decided_at: String,
}

/// An approval as the operator's device reads it. Only `id` is required: every
/// other field is optional on the wire, so a newer or older host cannot break
/// the feed.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Approval {
    pub id: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub seat_id: Option<String>,
    #[serde(default)]
    pub agent_label: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub action_digest: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub decided_by: Option<String>,
    #[serde(default)]
    pub decided_at: Option<String>,
    /// `lead`, `cto` or `user`; `None` once the policy decided it by itself.
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub escalated_from: Option<String>,
    #[serde(default)]
    pub decider_kind: Option<String>,
    #[serde(default)]
    pub decider_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    /// The concrete question when the request came for lack of information; it
    /// takes the place of the command on the card.
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub payload_redacted: Option<Value>,
    #[serde(default)]
    pub audit: Vec<ApprovalAudit>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ApprovalProject {
    pub project_id: String,
    pub mode: String,
}

/// The modes as the operator set them. A project without a row follows the
/// global mode.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ApprovalSettings {
    pub global: String,
    #[serde(default)]
    pub projects: Vec<ApprovalProject>,
}

/// The answer to a mode change. `escalated` counts the pending lead and CTO
/// approvals that the switch sent to the operator.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApprovalModeChange {
    pub settings: ApprovalSettings,
    #[serde(default)]
    pub replayed: bool,
    #[serde(default)]
    pub escalated: u64,
}

#[derive(Debug, Deserialize)]
struct ApprovalList {
    #[serde(default)]
    approvals: Vec<Approval>,
}

#[derive(Debug, Deserialize)]
struct ApprovalDecided {
    approval: Approval,
}

#[derive(Debug, Deserialize)]
struct SettingsEnvelope {
    settings: ApprovalSettings,
}

impl GatewayClient {
    /// The approvals of a conversation that still wait for the operator. The
    /// host answers the pending ones when no status is asked for.
    pub async fn chat_approvals(&self, conversation_id: &str) -> ChatResult<Vec<Approval>> {
        validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
        let list: ApprovalList = self
            .chat_get(
                "/v1/chat/approvals",
                &[("conversation_id", conversation_id.to_string())],
            )
            .await?;
        Ok(list.approvals)
    }

    /// The inbox: every approval that waits for this operator, whatever its
    /// conversation. A conversation-less one has no timeline line, so this is
    /// the only place it shows up.
    pub async fn chat_pending_approvals(&self) -> ChatResult<Vec<Approval>> {
        self.chat_approvals_with_status("pending").await
    }

    /// The approvals of one status. A device has no read of a single approval, so
    /// one that left the inbox is found by the status it ended in: approved,
    /// denied, expired or consumed.
    pub async fn chat_approvals_with_status(&self, status: &str) -> ChatResult<Vec<Approval>> {
        let list: ApprovalList = self
            .chat_get("/v1/chat/approvals", &[("status", status.to_string())])
            .await?;
        Ok(list.approvals)
    }

    /// Answers one approval once. `client_id` makes a retry safe: the host
    /// gives the same answer to the same id.
    pub async fn chat_approval_decide(
        &self,
        approval_id: &str,
        decision: &str,
        client_id: &str,
    ) -> ChatResult<Approval> {
        validate_prefixed_id(approval_id, "apr_", "aprobación")?;
        let body = decision_body(decision, client_id)?;
        let decided: ApprovalDecided = self
            .chat_send_json(
                Method::POST,
                &format!("/v1/chat/approvals/{approval_id}/decision"),
                &[],
                &body,
                true,
            )
            .await?;
        Ok(decided.approval)
    }

    pub async fn chat_approval_settings(&self) -> ChatResult<ApprovalSettings> {
        let envelope: SettingsEnvelope = self.chat_get("/v1/chat/approvals/settings", &[]).await?;
        Ok(envelope.settings)
    }

    /// Sets the global mode, or the mode of one project when `project_id` is
    /// given.
    pub async fn chat_set_approval_mode(
        &self,
        client_id: &str,
        mode: &str,
        project_id: Option<&str>,
    ) -> ChatResult<ApprovalModeChange> {
        let body = mode_body(client_id, mode, project_id)?;
        self.chat_send_json(
            Method::POST,
            "/v1/chat/approvals/settings",
            &[],
            &body,
            true,
        )
        .await
    }
}

fn decision_body(decision: &str, client_id: &str) -> ChatResult<Value> {
    validate_client_id(client_id)?;
    if !matches!(decision, "approve" | "deny") {
        return Err(ChatFailure::Invalid {
            message: "La decisión debe ser permitir o denegar.".to_string(),
            field: Some("decision".to_string()),
        });
    }
    Ok(json!({ "decision": decision, "scope": "once", "client_id": client_id }))
}

fn mode_body(client_id: &str, mode: &str, project_id: Option<&str>) -> ChatResult<Value> {
    validate_client_id(client_id)?;
    if !APPROVAL_MODES.contains(&mode) {
        return Err(ChatFailure::Invalid {
            message: "El modo de aprobación no es válido.".to_string(),
            field: Some("mode".to_string()),
        });
    }
    let mut body = Map::new();
    body.insert(
        "client_id".to_string(),
        Value::String(client_id.to_string()),
    );
    body.insert("mode".to_string(), Value::String(mode.to_string()));
    if let Some(project_id) = project_id {
        validate_prefixed_id(project_id, "prj_", "proyecto")?;
        body.insert(
            "project_id".to_string(),
            Value::String(project_id.to_string()),
        );
    }
    Ok(Value::Object(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_names_its_scope_and_client() {
        let body = decision_body("approve", "c-1").expect("valid");
        assert_eq!(body["decision"], "approve");
        assert_eq!(body["scope"], "once");
        assert_eq!(body["client_id"], "c-1");
    }

    #[test]
    fn only_approve_and_deny_are_decisions() {
        assert!(matches!(
            decision_body("always", "c-1"),
            Err(ChatFailure::Invalid { .. })
        ));
        assert!(matches!(
            decision_body("approve", ""),
            Err(ChatFailure::Invalid { .. })
        ));
    }

    #[test]
    fn a_global_mode_change_has_no_project() {
        let body = mode_body("c-2", "always_user", None).expect("valid");
        assert_eq!(body["mode"], "always_user");
        assert!(body.get("project_id").is_none());
    }

    #[test]
    fn a_project_mode_change_names_the_project() {
        let body = mode_body("c-3", "delegated", Some("prj_demo")).expect("valid");
        assert_eq!(body["project_id"], "prj_demo");
    }

    #[test]
    fn an_unknown_mode_never_leaves_the_client() {
        assert!(matches!(
            mode_body("c-4", "siempre", None),
            Err(ChatFailure::Invalid { field: Some(field), .. }) if field == "mode"
        ));
        assert!(matches!(
            mode_body("c-4", "delegated", Some("proyecto")),
            Err(ChatFailure::Invalid { .. })
        ));
    }

    #[test]
    fn a_host_row_with_only_an_id_still_reads() {
        let row: Approval = serde_json::from_value(json!({ "id": "apr_01J0" })).expect("tolerant");
        assert_eq!(row.id, "apr_01J0");
        assert_eq!(row.status, "");
        assert!(row.audit.is_empty());
    }

    #[test]
    fn a_settled_row_keeps_its_status_and_the_deadline() {
        let row: Approval = serde_json::from_value(json!({
            "id": "apr_01J1",
            "status": "expired",
            "expires_at": "2026-10-23T10:12:00.000Z"
        }))
        .expect("row");
        assert_eq!(row.id, "apr_01J1");
        assert_eq!(row.status, "expired");
        assert_eq!(row.expires_at, "2026-10-23T10:12:00.000Z");
    }

    #[test]
    fn a_full_host_row_keeps_the_fields_the_card_draws() {
        let row: Approval = serde_json::from_value(json!({
            "id": "apr_01J1",
            "seat_id": "ag_bob",
            "level": "user",
            "question": "¿Qué rama usar?",
            "risk": "medium",
            "status": "pending",
            "action_digest": "sha256:9f86",
            "payload_redacted": { "command": "cargo test" },
            "audit": [{ "device_id": "dev_1", "decision": "approve", "scope": "once", "decided_at": "t", "action_digest": "sha256:9f86" }]
        }))
        .expect("full row");
        assert_eq!(row.level.as_deref(), Some("user"));
        assert_eq!(row.question.as_deref(), Some("¿Qué rama usar?"));
        assert_eq!(row.audit[0].decision, "approve");
        assert_eq!(
            row.payload_redacted
                .as_ref()
                .and_then(|p| p["command"].as_str()),
            Some("cargo test")
        );
    }
}
