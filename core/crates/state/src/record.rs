//! The stored record of one supervised agent.

use crate::payload::TriageRow;
use crate::status::Status;

#[derive(Clone, Debug)]
pub struct AgentRecord {
    pub pane_id: String,
    pub workspace_id: String,
    pub project: String,
    pub agent: String,
    pub cwd: String,
    /// The operator's name for this conversation, when one was given. It wins
    /// over the terminal title in the row's display title.
    pub label: Option<String>,
    /// The terminal's own title (OSC 0/2, or a hook's message).
    pub terminal_title: Option<String>,
    /// The detected screen state (`idle`/`working`/`blocked`/`unknown`).
    pub status: Status,
    /// Operator has acknowledged the finished state. herdr's rules: set on
    /// any non-idle state; cleared on a completion transition
    /// (working/blocked → idle) unless the operator was watching.
    pub seen: bool,
    pub state_change_seq: u64,
    pub can_resume: bool,
    pub focused: bool,
    /// True when this pane comes from a previous host run (its process is
    /// gone; only its last screen survives). Additive contract field.
    pub restored: bool,
}

impl AgentRecord {
    /// The status the contract reports: `done` is derived, not stored
    /// (`idle` + not seen, exactly herdr's `pane_agent_status`).
    pub fn effective_status(&self) -> Status {
        if self.status == Status::Idle && !self.seen {
            Status::Done
        } else {
            self.status
        }
    }

    pub fn to_row(&self) -> TriageRow {
        let status = self.effective_status();
        TriageRow {
            pane_id: self.pane_id.clone(),
            workspace_id: self.workspace_id.clone(),
            project: self.project.clone(),
            agent: self.agent.clone(),
            status,
            attention: status.attention(),
            title: self.label.clone().or_else(|| self.terminal_title.clone()),
            cwd: self.cwd.clone(),
            focused: self.focused,
            can_resume: self.can_resume,
            restored: self.restored,
        }
    }
}
