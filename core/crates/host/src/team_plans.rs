//! The plans of the team (F3): an agent proposes its whole job and reads the
//! operator's answer. The store checks the steps; this file keeps every call to
//! the conversation of the seat that asks.

use excubia_chat::{NewPlan, Plan, PlanStep, SeatActor, ToolError, ToolReply};
use serde_json::json;

use super::team::TeamService;
use super::team_errors::tool_chat_error;

impl TeamService {
    pub(super) fn propose_plan(
        &self,
        actor: &SeatActor,
        steps: Vec<PlanStep>,
    ) -> Result<ToolReply, ToolError> {
        if !matches!(actor.role_id.as_str(), "alfred" | "lead") {
            return Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "only Alfred and the leads propose plans",
            ));
        }
        let plan = self
            .chat
            .propose_plan(NewPlan {
                conversation_id: actor.conversation_id.clone(),
                proposer_agent_id: actor.agent_id.clone(),
                steps,
            })
            .map_err(tool_chat_error)?;
        Ok(reply(&plan))
    }

    pub(super) fn read_plan(
        &self,
        actor: &SeatActor,
        plan_id: String,
    ) -> Result<ToolReply, ToolError> {
        let plan = self.chat.plan(&plan_id).map_err(tool_chat_error)?;
        if plan.conversation_id != actor.conversation_id {
            return Err(ToolError::invalid(
                "no_such_plan",
                "plan_id",
                "the plan is not in this conversation",
            ));
        }
        Ok(reply(&plan))
    }
}

fn reply(plan: &Plan) -> ToolReply {
    ToolReply {
        result: json!({
            "plan_id": plan.id,
            "status": plan.status,
            "summary": plan.summary,
            "note": plan.note,
            "delegation_id": plan.delegation_id,
        }),
    }
}
