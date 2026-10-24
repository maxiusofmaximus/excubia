//! CORE7 store rules: the limits of a project and of a seat, the budgets of
//! a turn and of an hour, and what a restart does to a running delegation.

mod common;

use common::{team, temp_dir};
use hn_core_chat::{
    ChatError, ChatStore, DelegationState, NewConversation, NewDelegation, NewProject, NewSeat,
};
#[test]
fn a_project_has_one_lead_and_eight_seats() {
    let t = team("limits");
    let error = t
        .store
        .create_seat(NewSeat {
            project_id: t.project.clone(),
            name: "Otro lead".to_string(),
            role_id: "lead".to_string(),
        })
        .expect_err("one lead per project");
    assert!(matches!(error, ChatError::LeadExists(_)), "{error}");

    // The lead and Ana already count: six more seats fill the project.
    for index in 0..6 {
        t.store
            .create_seat(NewSeat {
                project_id: t.project.clone(),
                name: format!("Extra {index}"),
                role_id: "specialist".to_string(),
            })
            .expect("seat");
    }
    let error = t
        .store
        .create_seat(NewSeat {
            project_id: t.project.clone(),
            name: "Demasiado".to_string(),
            role_id: "specialist".to_string(),
        })
        .expect_err("seat limit");
    assert!(matches!(error, ChatError::SeatLimit), "{error}");
}

#[test]
fn twelve_delegations_per_turn_are_allowed_and_the_thirteenth_is_not() {
    let t = team("turn-budget");
    for index in 0..12 {
        let delegation = t
            .assign(&t.alfred, &t.lead, &format!("Tarea {index}"), "run_1", None)
            .expect("delegation");
        t.store
            .complete_delegation(&delegation.id, "hecho")
            .expect("complete");
    }
    let error = t
        .assign(&t.alfred, &t.lead, "Tarea 13", "run_1", None)
        .expect_err("turn budget");
    assert!(matches!(error, ChatError::TurnBudget), "{error}");

    // Another turn of the same agent has its own budget.
    t.assign(&t.alfred, &t.lead, "Tarea 13", "run_2", None)
        .expect("another turn");
}

#[test]
fn sixty_delegations_per_hour_are_allowed_and_the_sixty_first_is_not() {
    let t = team("hour-budget");
    for index in 0..60 {
        let delegation = t
            .assign(
                &t.alfred,
                &t.lead,
                &format!("Tarea {index}"),
                &format!("run_{index}"),
                None,
            )
            .expect("delegation");
        t.store
            .complete_delegation(&delegation.id, "hecho")
            .expect("complete");
    }
    let error = t
        .assign(&t.alfred, &t.lead, "Tarea 60", "run_60", None)
        .expect_err("hour budget");
    assert!(matches!(error, ChatError::HourBudget), "{error}");
}

#[test]
fn a_running_delegation_is_interrupted_after_a_reopen() {
    let dir = temp_dir("restart");
    let path = dir.join("chat.db");
    let delegation_id = {
        let store = ChatStore::open(&path).expect("open");
        let alfred = store.alfred_id().expect("alfred");
        let project = store
            .create_project(NewProject {
                name: "Demo".to_string(),
                root_path: dir.join("demo"),
            })
            .expect("project");
        let lead = project.lead.expect("lead").agent_id;
        store.set_seat_pane(&lead, Some("pane_lead")).expect("pane");
        let conversation = store
            .create_conversation(NewConversation {
                title: Some("Trabajo".to_string()),
                project_id: Some(project.id.clone()),
            })
            .expect("conversation");
        let delegation = store
            .create_delegation(NewDelegation {
                conversation_id: conversation.id,
                from_agent_id: alfred,
                to_agent_id: lead,
                task: "Planifica".to_string(),
                turn_key: "run_1".to_string(),
                parent_id: None,
            })
            .expect("delegation");
        store.start_delegation(&delegation.id).expect("running");
        delegation.id
    };

    let reopened = ChatStore::open(&path).expect("reopen");
    let delegation = reopened.delegation(&delegation_id).expect("delegation");
    assert_eq!(delegation.state, DelegationState::Interrupted);
    let detail = reopened.delegation_detail(&delegation_id).expect("detail");
    assert_eq!(
        detail.card.map(|card| card.status),
        Some("interrupted".to_string())
    );
    assert_eq!(
        reopened
            .active_delegations_for_conversation(&detail.delegation.conversation_id)
            .expect("active")
            .len(),
        0,
        "an interrupted delegation is not active"
    );
}

#[test]
fn a_conversation_belongs_to_one_project() {
    let t = team("conversation-project");
    let scratch = temp_dir("conversation-project");
    let other = t
        .store
        .create_project(NewProject {
            name: "Otro".to_string(),
            root_path: scratch.join("otro"),
        })
        .expect("other project");
    let conversation = t
        .store
        .create_conversation(NewConversation {
            title: Some("Suelta".to_string()),
            project_id: None,
        })
        .expect("conversation");
    t.store
        .set_conversation_project(&conversation.id, &t.project)
        .expect("attach");
    assert_eq!(
        t.store
            .conversation_project(&conversation.id)
            .expect("project"),
        Some(t.project.clone())
    );
    let error = t
        .store
        .set_conversation_project(&conversation.id, &other.id)
        .expect_err("one project per conversation");
    assert!(matches!(error, ChatError::ScopeDenied), "{error}");
}
