//! A held wait (the GitHub code on github.com) counts as a pending wait of its run while it
//! is on, and the budget it pauses resumes with the balance it kept.

use std::time::{Duration, Instant};

use serde_json::json;

use crate::budget::ActiveBudget;
use crate::model_approval::{ApprovalKind, Decision, NewApproval, Risk};
use crate::ChatStore;

fn ask(store: &ChatStore, run_id: &str) -> String {
    store
        .create_approval(NewApproval {
            conversation_id: None,
            seat_id: None,
            agent_label: None,
            run_id: Some(run_id.to_string()),
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "github.link".to_string(),
            summary: "¿Enlazar GitHub con Excubia?".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: Some(Risk::High),
            action_digest: "sha256:github".to_string(),
            payload_redacted: Some(json!({ "host": "github.com" })),
            ttl_seconds: 600,
        })
        .expect("approval")
        .id
}

fn at(start: Instant, seconds: u64) -> Instant {
    start + Duration::from_secs(seconds)
}

#[test]
fn a_held_wait_pauses_the_run_only_while_it_is_on() {
    let store = ChatStore::in_memory().expect("store");
    let id = ask(&store, "run_1");
    // The card is decided: from here only the hold can wait on the operator's code.
    store
        .decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("decided");
    let mine = || store.approval_waits(&["run_1".to_string()]).expect("waits").pending;
    assert_eq!(mine(), 0);
    {
        let mut hold = store.hold_approval_wait(&id).expect("hold");
        hold.set(true);
        hold.set(true);
        assert_eq!(mine(), 1, "the hold counts once, however often it is set");
        hold.set(false);
        assert_eq!(mine(), 0);
        hold.set(true);
        assert_eq!(mine(), 1);
    }
    assert_eq!(mine(), 0, "a dropped hold releases the run");
}

#[test]
fn a_login_longer_than_the_budget_does_not_run_the_turn_out() {
    let store = ChatStore::in_memory().expect("store");
    let id = ask(&store, "run_1");
    store
        .decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("decided");
    let start = Instant::now();
    let mut budget = ActiveBudget::new(start, Duration::from_secs(600));
    let mut hold = store.hold_approval_wait(&id).expect("hold");

    // The code is shown at 10 s and the operator takes 700 s: the balance does not run out.
    budget.set_waiting(at(start, 10), true);
    hold.set(true);
    assert_eq!(budget.deadline(), None);
    assert!(!budget.is_exhausted(at(start, 700)));

    // The login ends at 700 s: 590 s of the balance are left, not a fresh 600.
    hold.set(false);
    let pending = store.approval_waits(&["run_1".to_string()]).expect("waits").pending;
    budget.set_waiting(at(start, 700), pending > 0);
    assert_eq!(budget.deadline(), Some(at(start, 700 + 590)));
}

#[test]
fn a_hold_on_an_unknown_approval_holds_nothing() {
    let store = ChatStore::in_memory().expect("store");
    let mut hold = store.hold_approval_wait("apr_missing").expect("hold");
    hold.set(true);
    assert_eq!(
        store
            .approval_waits(&["run_1".to_string()])
            .expect("waits")
            .pending,
        0
    );
}
