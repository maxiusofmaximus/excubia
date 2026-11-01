//! Shared fixtures of the delegation tests: a project with its lead, one
//! specialist and a conversation inside it.

#![allow(dead_code)]

use std::path::PathBuf;

use hn_core_chat::{
    Approval, ApprovalStatus, ChatError, ChatStore, Decision, Delegation, NewConversation,
    NewDelegation, NewProject, NewSeat, SeatActor,
};
use hn_core_test_dir::TestDir;

/// A project with its lead, one specialist and a conversation inside it.
pub struct Team {
    pub store: ChatStore,
    /// The database file, for the tests that read the audit trail directly.
    pub db: PathBuf,
    /// The project root: actions with this cwd are inside the project.
    pub root: PathBuf,
    pub alfred: String,
    pub lead: String,
    pub specialist: String,
    pub conversation: String,
    pub project: String,
    /// The scratch directory of the team: removed with it.
    pub dir: TestDir,
}

pub fn temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("core-team-tests-{name}"))
}

/// The project under test: `Demo` with Ernesto as lead and Ana as specialist.
pub fn team(name: &str) -> Team {
    let dir = temp_dir(name);
    let db = dir.join("chat.db");
    let store = ChatStore::open(&db).expect("open store");
    let alfred = store.alfred_id().expect("alfred");
    let root = dir.join("demo");
    let project = store
        .create_project(NewProject {
            name: "Demo".to_string(),
            root_path: root.clone(),
        })
        .expect("project");
    let lead = project
        .lead
        .as_ref()
        .map(|lead| lead.agent_id.clone())
        .expect("lead");
    assert_eq!(
        project.lead.as_ref().map(|lead| lead.name.clone()),
        Some("Ernesto".to_string())
    );
    let specialist = store
        .create_seat(NewSeat {
            project_id: project.id.clone(),
            name: "Ana".to_string(),
            role_id: "specialist".to_string(),
        })
        .expect("specialist");
    store
        .set_seat_pane(&lead, Some("pane_lead"))
        .expect("lead pane");
    store
        .set_seat_pane(&specialist.agent_id, Some("pane_ana"))
        .expect("specialist pane");
    let conversation = store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: Some(project.id.clone()),
        })
        .expect("conversation");
    Team {
        store,
        db,
        root,
        alfred,
        lead,
        specialist: specialist.agent_id,
        conversation: conversation.id,
        project: project.id,
        dir,
    }
}

/// A seat of the team, as the store sees the one that decides.
pub fn actor(agent_id: &str, role_id: &str, project_id: Option<&str>, team: &Team) -> SeatActor {
    SeatActor {
        agent_id: agent_id.to_string(),
        role_id: role_id.to_string(),
        project_id: project_id.map(str::to_string),
        conversation_id: team.conversation.clone(),
        run_id: String::new(),
    }
}

/// An approval the operator's device alone can decide: pending at `user`, with
/// the never-delegate reason.
pub fn assert_for_the_operator_alone(approval: &Approval) {
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.reason.as_deref(), Some("never_delegate"));
}

/// No seat of the team decides `approval`: neither the lead, the CTO nor a
/// worker, and the CTO's inbox does not list it.
pub fn assert_no_agent_decides(team: &Team, approval: &Approval) {
    let lead = actor(&team.lead, "lead", Some(&team.project), team);
    let alfred = actor(&team.alfred, "alfred", None, team);
    let worker = actor(&team.specialist, "specialist", Some(&team.project), team);
    for decider in [&lead, &alfred] {
        let refused = team
            .store
            .decide_delegated(decider, &approval.id, Decision::Approve, None);
        assert!(
            matches!(refused, Err(ChatError::ApprovalNotDelegable)),
            "{} decided the question: {refused:?}",
            decider.role_id
        );
    }
    let refused = team
        .store
        .decide_delegated(&worker, &approval.id, Decision::Approve, None);
    assert!(refused.is_err(), "a worker decided the question");
    let listed = team
        .store
        .pending_for_approver(&alfred)
        .expect("the CTO's inbox");
    assert!(
        listed.iter().all(|item| item.id != approval.id),
        "the CTO's inbox lists the question"
    );
}

impl Team {
    /// One delegation of the tree under test.
    pub fn assign(
        &self,
        from: &str,
        to: &str,
        task: &str,
        turn_key: &str,
        parent_id: Option<String>,
    ) -> Result<Delegation, ChatError> {
        self.store.create_delegation(NewDelegation {
            conversation_id: self.conversation.clone(),
            from_agent_id: from.to_string(),
            to_agent_id: to.to_string(),
            task: task.to_string(),
            turn_key: turn_key.to_string(),
            parent_id,
        })
    }
}
