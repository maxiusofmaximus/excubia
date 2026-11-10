//! `plan.propose` and `plan.status` (F3): an agent proposes the whole job before
//! its first step and reads the operator's answer. The steps are checked by the
//! chat store; here only the shape is read, and unknown fields are refused.

use excubia_core_chat::{PlanStep, ToolError, ToolRequest};
use serde::Deserialize;

use super::call::PlanPropose;
use super::parse::{arguments, name_field};
use super::ToolCall;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanProposeArgs {
    steps: Vec<PlanStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanStatusArgs {
    plan_id: String,
}

pub(super) fn propose(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: PlanProposeArgs = arguments(request, "steps")?;
    Ok(ToolCall::PlanPropose(PlanPropose { steps: args.steps }))
}

pub(super) fn status(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: PlanStatusArgs = arguments(request, "plan_id")?;
    Ok(ToolCall::PlanRead(name_field("plan_id", &args.plan_id)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_core_chat::Touch;
    use serde_json::{json, Value};

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    #[test]
    fn a_plan_keeps_its_steps_and_what_they_touch() {
        let call = propose(&request(
            "plan.propose",
            json!({"steps": [{"title": "Lee el repo", "touches": ["files"]}]}),
        ));
        assert_eq!(
            call,
            Ok(ToolCall::PlanPropose(PlanPropose {
                steps: vec![PlanStep {
                    title: "Lee el repo".to_string(),
                    touches: vec![Touch::Files],
                }],
            }))
        );
    }

    #[test]
    fn an_unknown_touch_or_field_is_refused_at_the_border() {
        let touch = propose(&request(
            "plan.propose",
            json!({"steps": [{"title": "Envia", "touches": ["postal"]}]}),
        ));
        assert_eq!(touch.expect_err("unknown touch").code, "invalid_arguments");

        let field = propose(&request(
            "plan.propose",
            json!({"steps": [], "owner": "lead"}),
        ));
        assert_eq!(field.expect_err("unknown field").code, "invalid_arguments");
    }

    #[test]
    fn the_status_names_one_plan() {
        assert_eq!(
            status(&request("plan.status", json!({"plan_id": "plan_1"}))),
            Ok(ToolCall::PlanRead("plan_1".to_string()))
        );
        assert!(status(&request("plan.status", json!({}))).is_err());
    }
}
