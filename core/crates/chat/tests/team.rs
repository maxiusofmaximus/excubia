//! CORE7 store rules: the delegation tree, the cards and the coalesced
//! synthesis of Alfred.

mod common;

use common::team;
use hn_core_chat::{ChatError, DelegationState};
#[test]
fn the_tree_is_alfred_lead_specialist() {
    let t = team("tree");
    let first = t
        .assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("alfred to lead");
    assert_eq!(first.depth, 1);
    assert_eq!(first.state, DelegationState::Queued);
    assert_eq!(first.from_name, "Alfred");
    assert_eq!(first.to_name, "Ernesto");

    let second = t
        .assign(
            &t.lead,
            &t.specialist,
            "Construye",
            &first.id,
            Some(first.id.clone()),
        )
        .expect("lead to specialist");
    assert_eq!(second.depth, 2);
    assert_eq!(second.parent_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(second.project_id.as_deref(), Some(t.project.as_str()));
}

#[test]
fn a_third_level_is_refused() {
    let t = team("depth");
    let first = t
        .assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("alfred to lead");
    let second = t
        .assign(
            &t.lead,
            &t.specialist,
            "Construye",
            &first.id,
            Some(first.id.clone()),
        )
        .expect("lead to specialist");
    let error = t
        .assign(
            &t.specialist,
            &t.lead,
            "Otra vez",
            &second.id,
            Some(second.id.clone()),
        )
        .expect_err("depth three");
    assert!(
        matches!(error, ChatError::DepthExceeded),
        "a specialist cannot delegate: {error}"
    );
}

#[test]
fn self_delegation_and_repeated_tasks_are_cycles() {
    let t = team("cycle");
    let error = t
        .assign(&t.lead, &t.lead, "Trabajo", "run_1", None)
        .expect_err("self delegation");
    assert!(matches!(error, ChatError::Cycle), "{error}");

    t.assign(&t.alfred, &t.lead, "Repite", "run_1", None)
        .expect("first");
    let error = t
        .assign(&t.alfred, &t.lead, "Repite", "run_2", None)
        .expect_err("same pair and task");
    assert!(matches!(error, ChatError::Cycle), "{error}");
}

#[test]
fn a_busy_assignee_is_refused() {
    let t = team("busy");
    t.assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("first");
    let busy = t
        .assign(&t.alfred, &t.lead, "Planifica otra vez", "run_2", None)
        .expect_err("the lead is busy");
    assert!(matches!(busy, ChatError::AssigneeBusy), "{busy}");
}

#[test]
fn alfred_only_assigns_leads() {
    let t = team("scope");
    let error = t
        .assign(&t.alfred, &t.specialist, "Construye", "run_1", None)
        .expect_err("Alfred cannot address a specialist");
    assert!(matches!(error, ChatError::ScopeDenied), "{error}");
}

#[test]
fn a_card_carries_the_delegation_and_the_agent() {
    let t = team("card");
    let delegation = t
        .assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("delegation");
    let page = t
        .store
        .timeline(&t.conversation, None, 10)
        .expect("timeline");
    assert_eq!(page.entries.len(), 1);
    let card = &page.entries[0];
    assert_eq!(card.kind, "card");
    assert_eq!(card.ref_id.as_deref(), Some(delegation.id.as_str()));
    assert_eq!(card.delegation_id.as_deref(), Some(delegation.id.as_str()));
    assert_eq!(card.status.as_deref(), Some("queued"));
    assert_eq!(card.pane_id.as_deref(), Some("pane_lead"));
    let links = card.links.as_ref().expect("links");
    assert_eq!(
        links.delegation,
        format!("/v1/chat/delegations/{}", delegation.id)
    );
    assert_eq!(links.agent.as_deref(), Some("/v1/agents/pane_lead"));
    assert!(card.summary.as_ref().is_some_and(|text| !text.is_empty()));
    assert!(card.title.as_ref().is_some_and(|text| !text.is_empty()));
}

#[test]
fn completing_a_delegation_writes_the_result_and_the_card_follows() {
    let t = team("complete");
    let delegation = t
        .assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("delegation");
    let message_id = t
        .store
        .complete_delegation(&delegation.id, "El plan está listo")
        .expect("complete");
    let detail = t.store.delegation_detail(&delegation.id).expect("detail");
    assert_eq!(detail.delegation.state, DelegationState::Done);
    let message = detail.result_message.expect("result message");
    assert_eq!(message.id, message_id);
    assert_eq!(message.body_md, "El plan está listo");
    let card = detail.card.expect("card");
    assert_eq!(card.status, "done");
    assert!(card.summary.contains("El plan está listo"));

    let error = t
        .store
        .complete_delegation(&delegation.id, "otra vez")
        .expect_err("already done");
    assert!(
        matches!(error, ChatError::DelegationNotActive(_)),
        "{error}"
    );
}

#[test]
fn the_synthesis_of_a_batch_is_one_turn() {
    let t = team("synthesis");
    let first = t
        .assign(&t.alfred, &t.lead, "Primera", "run_1", None)
        .expect("first");
    t.store
        .complete_delegation(&first.id, "resultado uno")
        .expect("complete");
    let second = t
        .assign(&t.alfred, &t.lead, "Segunda", "run_2", None)
        .expect("second");
    t.store
        .complete_delegation(&second.id, "resultado dos")
        .expect("complete");

    let pending = t.store.pending_synthesis(8).expect("pending");
    assert_eq!(pending.len(), 2);
    let turn = t
        .store
        .enqueue_synthesis(&pending, &t.alfred, "resumen")
        .expect("synthesis turn");
    assert_eq!(turn.agent_id, t.alfred);
    assert!(t.store.pending_synthesis(8).expect("pending").is_empty());
    for delegation in [&first, &second] {
        let detail = t.store.delegation_detail(&delegation.id).expect("detail");
        let synthesis = detail.synthesis.expect("synthesis");
        assert_eq!(synthesis.state, "queued");
        assert_eq!(synthesis.run_id.as_deref(), Some(turn.id.as_str()));
    }
}

#[test]
fn a_skipped_synthesis_is_not_retried() {
    let t = team("synthesis-skip");
    let delegation = t
        .assign(&t.alfred, &t.lead, "Primera", "run_1", None)
        .expect("first");
    t.store
        .complete_delegation(&delegation.id, "resultado")
        .expect("complete");
    let pending = t.store.pending_synthesis(8).expect("pending");
    t.store
        .skip_synthesis(&pending, "alfred_busy")
        .expect("skip");
    assert!(t.store.pending_synthesis(8).expect("pending").is_empty());
    let detail = t.store.delegation_detail(&delegation.id).expect("detail");
    assert_eq!(
        detail.synthesis.map(|synthesis| synthesis.state),
        Some("skipped".to_string())
    );
}

#[test]
fn a_blocked_pane_makes_the_delegation_wait() {
    let t = team("waiting");
    let delegation = t
        .assign(&t.alfred, &t.lead, "Necesita permiso", "run_1", None)
        .expect("delegation");
    t.store.start_delegation(&delegation.id).expect("running");
    assert!(t
        .store
        .delegation_state_from_pane("pane_lead", DelegationState::Waiting)
        .expect("waiting"));
    let detail = t.store.delegation_detail(&delegation.id).expect("detail");
    assert_eq!(detail.delegation.state, DelegationState::Waiting);
    let card = detail.card.expect("card");
    assert_eq!(card.status, "waiting");
    assert_eq!(card.attention, 3, "a waiting card asks for attention");
}

#[test]
fn cancelling_keeps_the_history() {
    let t = team("cancel");
    let delegation = t
        .assign(&t.alfred, &t.lead, "Planifica", "run_1", None)
        .expect("delegation");
    t.store.start_delegation(&delegation.id).expect("start");
    let cancelled = t.store.cancel_delegation(&delegation.id).expect("cancel");
    assert_eq!(cancelled.state, DelegationState::Cancelled);
    let detail = t.store.delegation_detail(&delegation.id).expect("detail");
    assert_eq!(
        detail.card.map(|card| card.status),
        Some("cancelled".to_string())
    );
    let error = t
        .store
        .cancel_delegation(&delegation.id)
        .expect_err("already cancelled");
    assert!(
        matches!(error, ChatError::DelegationNotActive(_)),
        "{error}"
    );
}

#[test]
fn the_kill_switch_cancels_every_active_delegation() {
    let t = team("kill-switch");
    let first = t
        .assign(&t.alfred, &t.lead, "Primera", "run_1", None)
        .expect("first");
    let second = t
        .assign(
            &t.lead,
            &t.specialist,
            "Segunda",
            &first.id,
            Some(first.id.clone()),
        )
        .expect("second");
    let cancelled = t
        .store
        .cancel_conversation(&t.conversation)
        .expect("kill switch");
    assert_eq!(cancelled.len(), 2);
    assert!(cancelled
        .iter()
        .all(|delegation| delegation.state == DelegationState::Cancelled));
    assert_eq!(
        t.store
            .active_delegations_for_conversation(&t.conversation)
            .expect("active")
            .len(),
        0
    );
    assert!(t.store.delegation(&first.id).is_ok());
    assert!(t.store.delegation(&second.id).is_ok());
}
