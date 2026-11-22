//! How the errors of the chat store become the typed codes of the MCP
//! surface: one place, so a code never changes between two tools.

use excubia_chat::{ChatError, ToolError};

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
        ChatError::InvalidPlan(_) => ToolError::invalid("invalid_plan", "steps", message),
        ChatError::NoSuchPlan(_) => ToolError::invalid("no_such_plan", "plan_id", message),
        ChatError::PlanRequired => ToolError::conflict("plan_required", message),
        ChatError::PlanNotApproved => ToolError::conflict("plan_not_approved", message),
        ChatError::PlanLinked => ToolError::conflict("plan_linked", message),
        ChatError::PlanDecided => ToolError::conflict("plan_decided", message),
        ChatError::ApprovalExpired => ToolError::conflict("approval_expired", message),
        ChatError::ApprovalAlreadyDecided => {
            ToolError::conflict("approval_already_decided", message)
        }
        ChatError::ApprovalLevelMismatch => ToolError::conflict("approval_level_mismatch", message),
        ChatError::ApprovalNotDelegable => ToolError::conflict("approval_not_delegable", message),
        ChatError::ApprovalSelfApproval => ToolError::conflict("approval_self_approval", message),
        ChatError::ApprovalBudget => ToolError::conflict("approval_budget", message),
        ChatError::NoteLimit => ToolError::conflict("note_limit", message),
        ChatError::NoteNotFound(_) => ToolError::invalid("not_found", "id", message),
        ChatError::ContactLimit => ToolError::conflict("contact_limit", message),
        ChatError::AddressLimit => ToolError::conflict("address_limit", message),
        ChatError::AliasLimit => ToolError::conflict("alias_limit", message),
        ChatError::InvalidAddress => ToolError::invalid("invalid_address", "addresses", message),
        ChatError::AddressTaken(_) => ToolError::conflict("address_taken", message),
        ChatError::AliasTaken => ToolError::conflict("alias_taken", message),
        ChatError::ContactNotFound(_) => ToolError::invalid("not_found", "id", message),
        ChatError::AlertRuleLimit => ToolError::conflict("rule_limit", message),
        ChatError::AlertRuleExists => ToolError::conflict("rule_exists", message),
        ChatError::AlertRuleNotFound(_) => ToolError::invalid("not_found", "id", message),
        _ => ToolError::conflict("chat_unavailable", message),
    }
}
