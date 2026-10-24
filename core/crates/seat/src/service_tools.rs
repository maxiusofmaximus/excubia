//! The tools a seat answers while its turn is open: the final reply, the
//! question for the user and the approval it asks for. The lifecycle of the
//! turn itself lives in [`crate::service`].

use hn_core_chat::{ToolError, ToolReply};
use serde_json::json;

use crate::approvals::{approval_for, approval_result, ApprovalAsk};
use crate::service::{lock, tool_chat_error, Phase, RunGrant, SeatService};
use crate::tools::ApprovalRequest;

impl SeatService {
    /// The final answer of the run. It closes the turn for good.
    pub(crate) fn reply(&self, grant: &RunGrant, text: String) -> Result<ToolReply, ToolError> {
        let mut phase = lock(&grant.phase);
        Self::check_phase(&phase)?;
        self.chat
            .complete_run_with_reply(&grant.run_id, &text)
            .map_err(tool_chat_error)?;
        *phase = Phase::Replied;
        grant.notify.notify_one();
        Ok(ToolReply {
            result: json!({ "status": "completed" }),
        })
    }

    /// A question for the user. It does not close the turn.
    pub(crate) fn ask(&self, grant: &RunGrant, question: String) -> Result<ToolReply, ToolError> {
        let phase = lock(&grant.phase);
        Self::check_phase(&phase)?;
        self.chat
            .append_agent_question(&grant.run_id, &question)
            .map_err(tool_chat_error)?;
        Ok(ToolReply {
            result: json!({ "status": "question_delivered" }),
        })
    }

    /// The run must still be open to answer anything.
    pub(crate) fn check_phase(phase: &Phase) -> Result<(), ToolError> {
        match phase {
            Phase::Running => Ok(()),
            Phase::Replied => Err(ToolError::conflict(
                "already_replied",
                "this run already has a final reply",
            )),
            Phase::Closed => Err(ToolError::conflict(
                "run_closed",
                "this run is no longer active",
            )),
        }
    }

    /// Open an approval for one action and wait for the operator. The call
    /// blocks and the agent only learns the answer.
    pub(crate) async fn ask_approval(
        &self,
        grant: &RunGrant,
        request: ApprovalRequest,
    ) -> Result<ToolReply, ToolError> {
        {
            let phase = lock(&grant.phase);
            Self::check_phase(&phase)?;
        }
        let approval = self
            .chat
            .request_approval(approval_for(
                &ApprovalAsk {
                    conversation_id: Some(grant.actor.conversation_id.clone()),
                    seat_id: Some(grant.actor.agent_id.clone()),
                    run_id: Some(grant.run_id.clone()),
                    pane_id: None,
                },
                &request,
            ))
            .await
            .map_err(tool_chat_error)?;
        approval_result(&approval)
    }
}
