//! The `alert.*` tools (F5): Alfred keeps the rules that decide which changes alert the user's
//! phone, and the mode they run in. No approval: a rule is reversible, and a rule only changes
//! what alerts, never what the agents do.

use excubia_chat::{AlertRule, AlertWriter, SeatActor, ToolError, ToolReply};
use excubia_seat::AlertCall;
use serde_json::json;

use super::team::TeamService;
use super::team_errors::tool_chat_error;

impl TeamService {
    pub(super) fn alert_tool(
        &self,
        actor: &SeatActor,
        call: AlertCall,
    ) -> Result<ToolReply, ToolError> {
        let by = AlertWriter::Seat(actor);
        let result = match call {
            AlertCall::RuleCreate(rule) => {
                let created = self
                    .chat
                    .create_alert_rule(by, rule)
                    .map_err(tool_chat_error)?;
                json!({ "rule": created.to_json() })
            }
            AlertCall::RuleList => {
                let mode = self.chat.alert_mode().map_err(tool_chat_error)?;
                let rules = self.chat.alert_rules().map_err(tool_chat_error)?;
                json!({
                    "mode": mode.as_str(),
                    "rules": rules.iter().map(AlertRule::to_json).collect::<Vec<_>>(),
                })
            }
            AlertCall::RuleUpdate { id, enabled } => {
                let updated = self
                    .chat
                    .set_alert_rule_enabled(by, &id, enabled)
                    .map_err(tool_chat_error)?;
                json!({ "rule": updated.to_json() })
            }
            AlertCall::RuleDelete { id } => {
                self.chat
                    .delete_alert_rule(by, &id)
                    .map_err(tool_chat_error)?;
                json!({ "id": id })
            }
            AlertCall::ModeSet(mode) => {
                let set = self
                    .chat
                    .set_alert_mode(by, mode)
                    .map_err(tool_chat_error)?;
                json!({ "mode": set.as_str() })
            }
        };
        Ok(ToolReply { result })
    }
}
