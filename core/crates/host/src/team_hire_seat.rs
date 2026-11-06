//! The seat a hire works in (F1C). A hire has no seat until a delegation goes to
//! it: then it gets one in the conversation's project, with a live panel, and
//! keeps that seat from then on.

use hn_core_chat::ToolError;

use super::team::TeamService;
use super::team_errors::tool_chat_error;

impl TeamService {
    /// The seat a delegation to `target` goes to. A seat id answers for itself;
    /// a hire id gets its seat on first use, and the same seat after that.
    pub(super) async fn seat_of_target(
        &self,
        target: &str,
        conversation_id: &str,
    ) -> Result<String, ToolError> {
        let Some(project_id) = self
            .chat
            .conversation_project(conversation_id)
            .map_err(tool_chat_error)?
        else {
            return Ok(target.to_string());
        };
        let Some(seat) = self
            .chat
            .seat_for_hire(target, &project_id)
            .map_err(tool_chat_error)?
        else {
            return Ok(target.to_string());
        };
        if seat.pane_id.is_none() {
            self.spawn_panel(&seat).await?;
        }
        Ok(seat.agent_id)
    }
}
