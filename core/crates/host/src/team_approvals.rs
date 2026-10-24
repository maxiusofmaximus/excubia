//! The blocking UAC of the team seats: the preflight that proves the
//! adapter works and the approval an agent asks for.
//!
//! Two rules hold here. Nothing decides: only a device with the `APPROVE`
//! grant moves an approval out of `pending`, so a message, another agent or
//! the agent itself can never approve anything. And nothing runs twice: the
//! digest is the identity of the action and the approval is consumed before
//! the tool returns.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hn_core_chat::{SeatActor, ToolError, ToolReply};
use hn_core_seat::{
    approval_for, approval_result, canary_plan, run_canary, ApprovalAsk, ApprovalDecide,
    ApprovalRequest, CliProfile, UacState,
};
use serde_json::json;

use super::team::TeamService;
use super::team_errors::tool_chat_error;

/// How long the UAC canary may take before the seat is refused. The first
/// canary of a host installs the OpenCode provider package in the shared
/// cache, which is the only slow part.
const CANARY_TIMEOUT: Duration = Duration::from_secs(60);

impl TeamService {
    /// Prove that the blocking UAC adapter of this CLI works before the seat
    /// gets a panel. A seat whose adapter failed is `unsafe`: it is visible
    /// in the bootstrap and it is never launched.
    pub(super) async fn verify_uac(
        &self,
        seat: &hn_core_chat::Seat,
        profile: &CliProfile,
    ) -> Result<(), ToolError> {
        let outcome = self
            .preflight(profile, &self.seat_dir(&seat.agent_id))
            .await;
        self.chat
            .set_seat_uac_state(&seat.agent_id, outcome.state.as_str())
            .map_err(tool_chat_error)?;
        if outcome.state == UacState::Unsafe {
            return Err(ToolError::conflict("uac_unsafe", &outcome.reason));
        }
        Ok(())
    }

    /// The canary of the configured CLI, or `unchecked` when there is no
    /// plugin to verify.
    async fn preflight(
        &self,
        profile: &CliProfile,
        work_dir: &Path,
    ) -> hn_core_seat::CanaryOutcome {
        let Some(plugin_dir) = self.uac_plugin_dir.clone() else {
            return hn_core_seat::CanaryOutcome {
                state: UacState::Unchecked,
                reason: "no UAC plugin is configured".to_string(),
            };
        };
        let Some(plan) = canary_plan(profile, &plugin_dir, work_dir, &self.canary_cache()).await
        else {
            return hn_core_seat::CanaryOutcome {
                state: UacState::Unchecked,
                reason: format!(
                    "{} has no blocking UAC adapter to verify",
                    profile.id.as_str()
                ),
            };
        };
        run_canary(&plan, CANARY_TIMEOUT).await
    }

    /// The working directory of a seat: the canary writes its isolated
    /// configuration there.
    fn seat_dir(&self, agent_id: &str) -> PathBuf {
        let dir = self.data_dir.join("seats").join(agent_id).join("resident");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// The provider package of the canary is installed once per host here.
    fn canary_cache(&self) -> PathBuf {
        let dir = self.data_dir.join("uac-canary");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Open an approval for one action and wait for the operator. The call
    /// blocks: the agent only learns the answer, it never gives it.
    pub(super) async fn request_approval(
        &self,
        actor: &SeatActor,
        pane_id: Option<String>,
        request: ApprovalRequest,
    ) -> Result<ToolReply, ToolError> {
        let approval = self
            .chat
            .request_approval(approval_for(
                &ApprovalAsk {
                    conversation_id: Some(actor.conversation_id.clone()),
                    seat_id: Some(actor.agent_id.clone()),
                    run_id: Some(actor.run_id.clone()),
                    pane_id,
                },
                &request,
            ))
            .await
            .map_err(tool_chat_error)?;
        approval_result(&approval)
    }

    /// A lead's or Alfred's answer to an approval waiting at its level. The
    /// store decides whether this seat may answer it; the reply is the approval
    /// as it stands after the answer.
    pub(super) fn decide_delegated(
        &self,
        actor: &SeatActor,
        decide: ApprovalDecide,
    ) -> Result<ToolReply, ToolError> {
        let approval = self
            .chat
            .decide_delegated(
                actor,
                &decide.approval_id,
                decide.decision,
                decide.reason.as_deref(),
            )
            .map_err(tool_chat_error)?;
        Ok(ToolReply {
            result: json!({ "approval": approval }),
        })
    }

    /// The approvals waiting at the level of this seat, in the shape the
    /// decision reply uses. The store scopes them: a lead gets its project's.
    pub(super) fn pending_approvals(&self, actor: &SeatActor) -> Result<ToolReply, ToolError> {
        let approvals = self
            .chat
            .pending_for_approver(actor)
            .map_err(tool_chat_error)?;
        Ok(ToolReply {
            result: json!({ "approvals": approvals }),
        })
    }
}
