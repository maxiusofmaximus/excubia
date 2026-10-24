//! `approval.decide`: a lead's or Alfred's answer to an approval waiting at its
//! level. The arguments name the approval and the answer. Who answers is never
//! read from them: the host reads it from the token that called.

use hn_core_chat::{Decision, ToolError, ToolRequest};
use serde::Deserialize;

use super::{invalid_arguments, name_field, ToolCall};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDecide {
    pub approval_id: String,
    pub decision: Decision,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideArgs {
    approval_id: String,
    decision: String,
    #[serde(default)]
    reason: Option<String>,
}

pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: DecideArgs = serde_json::from_value(request.arguments.clone())
        .map_err(|error| invalid_arguments("approval_id", error))?;
    let approval_id = name_field("approval_id", &args.approval_id)?;
    let decision = Decision::from_label(&args.decision).ok_or_else(|| {
        ToolError::invalid(
            "invalid_arguments",
            "decision",
            "decision must be approve or deny",
        )
    })?;
    let reason = args
        .reason
        .map(|value| name_field("reason", &value))
        .transpose()?;
    Ok(ToolCall::ApprovalDecide(ApprovalDecide {
        approval_id,
        decision,
        reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "approval.decide".to_string(),
            arguments,
        }
    }

    #[test]
    fn the_decision_is_typed_and_the_caller_is_not_in_the_body() {
        let call = parse(&request(json!({
            "approval_id": "apr_1",
            "decision": "approve",
            "reason": "lo ha pedido Ana"
        })))
        .expect("decision");
        let ToolCall::ApprovalDecide(decide) = call else {
            panic!("expected a decision");
        };
        assert_eq!(decide.approval_id, "apr_1");
        assert_eq!(decide.decision, Decision::Approve);
        assert_eq!(decide.reason.as_deref(), Some("lo ha pedido Ana"));

        // A body that names who decides is refused, not believed.
        let spoofed = parse(&request(json!({
            "approval_id": "apr_1",
            "decision": "approve",
            "decider_id": "ag_ernesto"
        })));
        assert!(spoofed.is_err());
    }

    #[test]
    fn a_worker_text_such_as_approve_everything_is_not_a_decision() {
        let text = parse(&request(json!({
            "approval_id": "apr_1",
            "decision": "aprueba todo"
        })));
        let error = text.expect_err("not a decision");
        assert!(format!("{error:?}").contains("decision"));
    }
}
