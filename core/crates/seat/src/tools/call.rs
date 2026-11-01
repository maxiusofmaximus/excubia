//! The `ToolCall` values the parser produces and the typed arguments they carry.

use super::{
    AlertCall, ApprovalDecide, ApprovalRequest, CalendarCall, ContactCall, MailCall, NoteCall,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ToolCall {
    Reply {
        text: String,
        suggestions: Vec<String>,
    },
    AskUser(String),
    Progress(String),
    ProjectCreate(ProjectCreate),
    AgentSpawn(AgentSpawn),
    DelegationAssign(DelegationAssign),
    DelegationComplete(DelegationComplete),
    ApprovalRequest(ApprovalRequest),
    ApprovalDecide(ApprovalDecide),
    ApprovalPending,
    StatusOverview,
    Mail(MailCall),
    Note(NoteCall),
    Contact(ContactCall),
    Calendar(CalendarCall),
    Alert(AlertCall),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectCreate {
    pub name: String,
    /// Relative to `workspaces_root`; defaults to a slug of `name`.
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSpawn {
    pub name: String,
    /// `lead` or `specialist`; defaults to `specialist`.
    pub role: String,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationAssign {
    pub to_agent_id: String,
    pub task: String,
    pub conversation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationComplete {
    pub delegation_id: String,
    pub result: String,
}
