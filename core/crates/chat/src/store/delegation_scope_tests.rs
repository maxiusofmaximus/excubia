//! The link between an approval and the task that asked for it. A delegation's
//! budget counts only the rows persisted for that task, and a pending row is never
//! moved from one task to another: it stays the permission context and the audit
//! of the task it was created for.

use std::path::PathBuf;

use serde_json::json;

use crate::model::{ConversationUpdate, SectionChange};
use crate::model_approval::{ApprovalKind, NewApproval, Risk};
use crate::model_team::{NewDelegation, NewProject};
use crate::{ChatError, ChatStore, NewConversation};

/// Alfred, a lead of one project and a conversation on that project.
struct Team {
    store: ChatStore,
    alfred: String,
    lead: String,
    conversation: String,
}

fn team() -> Team {
    let store = ChatStore::in_memory().expect("store");
    let alfred = store.alfred_id().expect("alfred");
    let project = store
        .create_project(NewProject {
            name: "Proyecto".to_string(),
            root_path: PathBuf::from(std::env::temp_dir()).join("excubia-scope-tests"),
        })
        .expect("project");
    // Creating a project already opens its lead.
    let lead = store
        .project_lead(&project.id)
        .expect("lead query")
        .expect("project lead")
        .agent_id;
    let conversation = store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: Some(project.id),
        })
        .expect("conversation")
        .id;
    Team {
        store,
        alfred,
        lead,
        conversation,
    }
}

/// Alfred assigns `task` to the lead; returns the delegation id.
fn assign(team: &Team, task: &str) -> Result<String, ChatError> {
    team.store
        .create_delegation(NewDelegation {
            conversation_id: team.conversation.clone(),
            from_agent_id: team.alfred.clone(),
            to_agent_id: team.lead.clone(),
            task: task.to_string(),
            turn_key: format!("turn-{task}"),
            parent_id: None,
            steps: 1,
            plan_id: None,
        })
        .map(|delegation| delegation.id)
}

fn digest(name: &str) -> String {
    format!("sha256:{name:0>64}")
}

/// A request as a resident panel opens it: the seat only, no conversation, no run.
fn resident_ask(team: &Team, digest: &str) -> Result<String, ChatError> {
    team.store
        .create_approval(NewApproval {
            conversation_id: None,
            seat_id: Some(team.lead.clone()),
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "git status".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: Some(Risk::High),
            action_digest: digest.to_string(),
            payload_redacted: Some(json!({ "command": "git status" })),
            ttl_seconds: 600,
        })
        .map(|approval| approval.id)
}

#[test]
fn a_resident_request_counts_on_the_task_open_for_its_seat() {
    let team = team();
    let first = assign(&team, "Revisa el estado del repositorio").expect("delegation");
    resident_ask(&team, &digest("a")).expect("approval");
    let waits = team.store.delegation_waits(&first).expect("waits");
    assert_eq!(waits.pending, 1);
    assert!(waits.next_change.is_some(), "the TTL is the next change");
}

#[test]
fn the_same_task_shares_its_pending_request() {
    let team = team();
    assign(&team, "Revisa el estado del repositorio").expect("delegation");
    let first = resident_ask(&team, &digest("b")).expect("approval");
    let again = resident_ask(&team, &digest("b")).expect("same request");
    assert_eq!(first, again);
    assert_eq!(team.store.pending_approval_count().expect("count"), 1);
}

#[test]
fn a_pending_request_of_a_finished_task_is_never_moved_to_the_next_one() {
    let team = team();
    let old = assign(&team, "Revisa el estado del repositorio").expect("delegation");
    let row = resident_ask(&team, &digest("c")).expect("approval");
    // The operator cancels the task; its CLI may still run, but its open card closes with it.
    team.store.cancel_delegation(&old).expect("cancel");

    let next = assign(&team, "Cuenta las ramas del repositorio").expect("next delegation");
    let refused = resident_ask(&team, &digest("c"));
    assert!(
        matches!(refused, Err(ChatError::ApprovalDigestReused)),
        "{refused:?}"
    );
    // The row keeps its task and is closed with it: the new task waits on nothing,
    // the old one waits on no open card, and no second row was opened.
    assert_eq!(team.store.delegation_waits(&next).expect("waits").pending, 0);
    assert_eq!(team.store.delegation_waits(&old).expect("waits").pending, 0);
    assert_eq!(team.store.pending_approval_count().expect("count"), 0);
    assert_eq!(
        team.store
            .approval(&row)
            .expect("row")
            .status
            .as_str(),
        "expired"
    );
}

#[test]
fn concurrent_identical_requests_of_one_task_open_one_row() {
    let team = team();
    assign(&team, "Revisa el estado del repositorio").expect("delegation");
    let ids: Vec<Result<String, ChatError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| resident_ask(&team, &digest("d"))))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect()
    });
    let first = ids[0].as_ref().expect("first request").clone();
    for id in &ids {
        assert_eq!(id.as_ref().expect("request"), &first);
    }
    assert_eq!(team.store.pending_approval_count().expect("count"), 1);
}

#[test]
fn a_request_of_an_archived_conversation_is_not_waited_on() {
    let team = team();
    let delegation = assign(&team, "Revisa el estado del repositorio").expect("delegation");
    team.store
        .create_approval(NewApproval {
            conversation_id: Some(team.conversation.clone()),
            seat_id: Some(team.lead.clone()),
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "git status".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: Some(Risk::High),
            action_digest: digest("e"),
            payload_redacted: Some(json!({ "command": "git status" })),
            ttl_seconds: 600,
        })
        .expect("approval");
    assert_eq!(team.store.delegation_waits(&delegation).expect("waits").pending, 1);
    team.store
        .update_conversation(
            &team.conversation,
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archived");
    assert_eq!(team.store.delegation_waits(&delegation).expect("waits").pending, 0);
}

#[test]
fn a_resident_request_with_no_open_task_is_refused() {
    let team = team();
    let old = assign(&team, "Revisa el estado del repositorio").expect("delegation");
    team.store.cancel_delegation(&old).expect("cancel");
    // No task is open for the seat: the request would have no task to close with it.
    let refused = resident_ask(&team, &digest("e"));
    assert!(
        matches!(refused, Err(ChatError::ApprovalScopeDenied)),
        "{refused:?}"
    );
    assert_eq!(team.store.pending_approval_count().expect("count"), 0);
}
