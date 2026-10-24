//! The MCP answer of `approval.request`.
//!
//! The operator decides and the agent only learns the answer: the tool call
//! blocks until the decision and it never carries a decision of its own.

use hn_core_chat::{
    action_digest, Approval, ApprovalStatus, NewApproval, ToolError, ToolReply, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

use crate::tools::ApprovalRequest;

/// Who asks for the approval.
#[derive(Clone, Debug, Default)]
pub struct ApprovalAsk {
    pub conversation_id: Option<String>,
    pub seat_id: Option<String>,
    pub run_id: Option<String>,
    pub pane_id: Option<String>,
}

/// The approval an agent asks for: the identity of the caller and the action.
/// The host computes the digest of the action, so the agent never chooses it.
pub fn approval_for(ask: &ApprovalAsk, request: &ApprovalRequest) -> NewApproval {
    let cwd = request.cwd.clone().unwrap_or_default();
    let digest = action_digest(
        request.kind.as_str(),
        &request.tool,
        &request.args,
        &cwd,
        &request.paths,
    );
    NewApproval {
        conversation_id: ask.conversation_id.clone().filter(|id| !id.is_empty()),
        seat_id: ask.seat_id.clone(),
        agent_label: None,
        run_id: ask.run_id.clone().filter(|id| !id.is_empty()),
        pane_id: ask.pane_id.clone(),
        kind: request.kind,
        tool: request.tool.clone(),
        summary: request.summary.clone(),
        cwd: request.cwd.clone(),
        paths: request.paths.clone(),
        risk: None,
        action_digest: digest,
        payload_redacted: Some(request.args.clone()),
        ttl_seconds: request.ttl_seconds.unwrap_or(DEFAULT_TTL_SECONDS),
    }
}

/// The reply of an allowed action, or the refusal of the others.
pub fn approval_result(approval: &Approval) -> Result<ToolReply, ToolError> {
    match approval.status {
        ApprovalStatus::Approved => Ok(ToolReply {
            result: json!({
                "approval_id": approval.id,
                "status": approval.status.as_str(),
            }),
        }),
        ApprovalStatus::Denied => Err(ToolError::conflict(
            "approval_denied",
            "the operator denied the action",
        )),
        ApprovalStatus::Expired => Err(ToolError::conflict(
            "approval_expired",
            "the approval expired before the operator answered",
        )),
        other => Err(ToolError::conflict(
            "approval_not_approved",
            format!("the approval is {}", other.as_str()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(status: ApprovalStatus) -> Approval {
        Approval {
            id: "apr_1".to_string(),
            conversation_id: None,
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: hn_core_chat::ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "cargo test".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: hn_core_chat::Risk::High,
            action_digest: "sha256:0".to_string(),
            status,
            scope: "once".to_string(),
            expires_at: "2026-10-22T10:00:00.000Z".to_string(),
            decided_by: None,
            decided_at: None,
            level: None,
            escalated_from: None,
            decider_kind: None,
            decider_id: None,
            reason: None,
            risk_score: None,
            scorer_version: None,
            created_at: "2026-10-22T09:58:00.000Z".to_string(),
            updated_at: "2026-10-22T09:58:00.000Z".to_string(),
            payload_redacted: None,
            audit: Vec::new(),
        }
    }

    #[test]
    fn only_an_approved_action_may_run() {
        let allowed = approval_result(&approval(ApprovalStatus::Approved)).expect("allowed");
        assert_eq!(allowed.result["status"], "approved");
        let denied = approval_result(&approval(ApprovalStatus::Denied)).expect_err("denied");
        assert_eq!(denied.code, "approval_denied");
        let expired = approval_result(&approval(ApprovalStatus::Expired)).expect_err("expired");
        assert_eq!(expired.code, "approval_expired");
        let pending = approval_result(&approval(ApprovalStatus::Pending)).expect_err("pending");
        assert_eq!(pending.code, "approval_not_approved");
    }
}
