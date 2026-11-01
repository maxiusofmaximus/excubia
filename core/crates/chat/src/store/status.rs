//! What `status.overview` reads from the chat, in one place: the open
//! delegations, the approvals at the caller's level, the projects and the seat
//! panes. The tool and the bootstrap read the same rows, so their counts agree.

use crate::error::ChatError;
use crate::model::ProjectSummary;
use crate::model_approval::{Approval, ApprovalKind};
use crate::model_team::{Delegation, DelegationState};
use crate::seat::SeatActor;

use super::delegation_rules::{bounded, delegation_row};
use super::delegations::{DELEGATION_SELECT, TASK_CHARS};

/// The longest name the overview prints. With the 120-character tasks and the
/// 20-item lists this keeps the whole answer well under 16 KiB.
pub const LABEL_CHARS: usize = 60;

/// An open delegation as the overview shows it: the task bounded to 120
/// characters.
#[derive(Clone, Debug)]
pub struct StatusDelegation {
    pub id: String,
    pub state: DelegationState,
    pub task: String,
    pub to_name: String,
}

/// An approval waiting at the caller's level, without its payload.
#[derive(Clone, Debug)]
pub struct StatusApproval {
    pub id: String,
    pub kind: ApprovalKind,
    pub agent: String,
}

/// The chat facts of the overview. Pane ids stay in `panes`: the host joins
/// them with the panels and never returns them.
#[derive(Clone, Debug)]
pub struct StatusFacts {
    pub chat_revision: i64,
    pub projects: Vec<ProjectSummary>,
    /// `(pane_id, project_id)` of every seat that owns a pane.
    pub panes: Vec<(String, String)>,
    pub delegations: Vec<StatusDelegation>,
    pub approvals: Vec<StatusApproval>,
    pub last_activity: Option<String>,
}

impl super::ChatStore {
    /// The overview's chat facts, as `approver` may see them.
    pub fn status_facts(&self, approver: &SeatActor) -> Result<StatusFacts, ChatError> {
        let bootstrap = self.bootstrap()?;
        let delegations = self.open_delegations()?;
        let approvals = self.pending_for_approver(approver)?;
        Ok(StatusFacts {
            chat_revision: bootstrap.chat.revision,
            projects: bootstrap
                .projects
                .into_iter()
                .map(|project| ProjectSummary {
                    name: bounded(&project.name, LABEL_CHARS),
                    root_label: bounded(&project.root_label, LABEL_CHARS),
                    ..project
                })
                .collect(),
            panes: self.seat_panes()?,
            delegations: delegations
                .into_iter()
                .map(|delegation| self.status_delegation(delegation))
                .collect(),
            approvals: approvals
                .into_iter()
                .map(|approval| self.status_approval(approval))
                .collect(),
            last_activity: self.last_activity()?,
        })
    }

    fn status_delegation(&self, delegation: Delegation) -> StatusDelegation {
        StatusDelegation {
            id: delegation.id,
            state: delegation.state,
            task: bounded(&delegation.task, TASK_CHARS),
            to_name: bounded(&delegation.to_name, LABEL_CHARS),
        }
    }

    /// The agent is the label the CLI gave it, or the name of its seat.
    fn status_approval(&self, approval: Approval) -> StatusApproval {
        let agent = approval.agent_label.or_else(|| {
            let seat_id = approval.seat_id.as_deref()?;
            self.seat(seat_id).ok().map(|seat| seat.name)
        });
        StatusApproval {
            id: approval.id,
            kind: approval.kind,
            agent: bounded(&agent.unwrap_or_default(), LABEL_CHARS),
        }
    }

    fn open_delegations(&self) -> Result<Vec<Delegation>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(&format!(
            "{DELEGATION_SELECT}
              WHERE d.state IN ('queued', 'running', 'waiting')
              ORDER BY d.created_at ASC, d.id ASC"
        ))?;
        let rows = statement
            .query_map([], delegation_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn seat_panes(&self) -> Result<Vec<(String, String)>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT pane_id, project_id FROM agent_seat
              WHERE pane_id IS NOT NULL AND project_id IS NOT NULL",
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn last_activity(&self) -> Result<Option<String>, ChatError> {
        let connection = self.lock();
        let last: Option<String> =
            connection.query_row("SELECT MAX(created_at) FROM chat_event", [], |row| {
                row.get(0)
            })?;
        Ok(last)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::ChatStore;
    use crate::NewProject;

    use super::*;

    #[test]
    fn a_long_project_name_is_bounded_in_the_overview() {
        let store = ChatStore::in_memory().expect("store");
        store
            .create_project(NewProject {
                name: "Proyecto ".repeat(40),
                root_path: "C:\\work\\demo".into(),
            })
            .expect("project");
        let alfred = SeatActor {
            agent_id: store.alfred_id().expect("alfred"),
            role_id: "alfred".to_string(),
            project_id: None,
            conversation_id: String::new(),
            run_id: String::new(),
        };
        let facts = store.status_facts(&alfred).expect("facts");
        let name = &facts.projects[0].name;
        assert_eq!(name.chars().count(), LABEL_CHARS, "{name}");
        assert!(name.ends_with('…'), "{name}");
        assert_eq!(facts.projects[0].root_label, "demo");
    }
}
