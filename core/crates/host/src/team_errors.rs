//! How the errors of the chat store become the typed codes of the MCP
//! surface: one place, so a code never changes between two tools.

use hn_core_chat::{ChatError, ToolError};

pub(super) fn tool_chat_error(error: ChatError) -> ToolError {
    let message = error.to_string();
    match error {
        ChatError::ScopeDenied => ToolError::invalid("scope_denied", "tool", message),
        ChatError::DepthExceeded => ToolError::invalid("depth_exceeded", "tool", message),
        ChatError::Cycle => ToolError::invalid("cycle_detected", "tool", message),
        ChatError::UnknownProject(_) => ToolError::invalid("unknown_project", "tool", message),
        ChatError::NoSuchSeat(_) => ToolError::invalid("unknown_agent", "tool", message),
        ChatError::NoSuchApproval => ToolError::invalid("no_such_approval", "approval_id", message),
        ChatError::ApprovalScopeDenied => {
            ToolError::forbidden("approval_scope_denied", "approval_id", message)
        }
        ChatError::NoSuchDelegation(_) => ToolError::conflict("no_such_delegation", message),
        ChatError::DelegationNotActive(_) => ToolError::conflict("delegation_cancelled", message),
        ChatError::NoSuchConversation => ToolError::conflict("no_such_conversation", message),
        ChatError::LeadExists(_) => ToolError::conflict("lead_exists", message),
        ChatError::ProjectExists => ToolError::conflict("project_exists", message),
        ChatError::SeatLimit => ToolError::conflict("seat_limit", message),
        ChatError::AssigneeBusy | ChatError::ActorBusy => {
            ToolError::conflict("agent_busy", message)
        }
        ChatError::TurnBudget => ToolError::conflict("turn_budget", message),
        ChatError::HourBudget => ToolError::conflict("hour_budget", message),
        ChatError::ApprovalExpired => ToolError::conflict("approval_expired", message),
        ChatError::ApprovalAlreadyDecided => {
            ToolError::conflict("approval_already_decided", message)
        }
        ChatError::ApprovalLevelMismatch => ToolError::conflict("approval_level_mismatch", message),
        ChatError::ApprovalNotDelegable => ToolError::conflict("approval_not_delegable", message),
        ChatError::ApprovalSelfApproval => ToolError::conflict("approval_self_approval", message),
        ChatError::ApprovalBudget => ToolError::conflict("approval_budget", message),
        _ => ToolError::conflict("chat_unavailable", message),
    }
}
