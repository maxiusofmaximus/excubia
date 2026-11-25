//! The TTL clock of an approval (v29): it starts when an authorized UI first
//! shows the card, once. Before that nothing expires; a late app cannot expire
//! a card it never showed, and a second showing never renews the clock.

use std::sync::Arc;

use serde_json::json;

use crate::clock::ManualClock;
use crate::model_approval::{ApprovalKind, ApprovalStatus, Decision, NewApproval, Risk};
use crate::ChatStore;

/// 2026-11-25T12:00:00Z, the host's clock at the start of each test.
const START_MS: i64 = 1_795_608_000_000;

fn timed() -> (ChatStore, Arc<ManualClock>) {
    let store = ChatStore::in_memory().expect("store");
    let clock = Arc::new(ManualClock::at(START_MS));
    store.set_clock(clock.clone());
    (store, clock)
}

fn ask(store: &ChatStore, digest: &str, ttl_seconds: u32) -> String {
    store
        .create_approval(NewApproval {
            conversation_id: None,
            seat_id: None,
            agent_label: None,
            run_id: Some("run_clock".to_string()),
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "cargo test".to_string(),
            cwd: None,
            paths: Vec::new(),
            risk: Some(Risk::High),
            action_digest: digest.to_string(),
            payload_redacted: Some(json!({ "command": "cargo test" })),
            ttl_seconds,
        })
        .expect("approval")
        .id
}

fn status(store: &ChatStore, id: &str) -> ApprovalStatus {
    store.approval(id).expect("approval").status
}

#[test]
fn a_card_shown_after_its_ttl_was_over_is_not_expired_and_gets_the_full_ttl() {
    let (store, clock) = timed();
    let id = ask(&store, "sha256:late-app", 60);

    // The app opens late: the TTL would have ended, but nobody has seen the card.
    clock.advance_secs(300);
    assert_eq!(status(&store, &id), ApprovalStatus::Pending);
    assert!(store.approval(&id).expect("approval").shown_at.is_none());

    let shown = store.show_approval(&id).expect("shown");
    assert_eq!(shown.status, ApprovalStatus::Pending);
    assert_eq!(shown.shown_at.as_deref(), Some("2026-11-25T12:05:00.000Z"));
    assert_eq!(shown.expires_at, "2026-11-25T12:06:00.000Z");

    clock.advance_secs(59);
    assert_eq!(status(&store, &id), ApprovalStatus::Pending);
    clock.advance_secs(1);
    assert_eq!(status(&store, &id), ApprovalStatus::Expired);
}

#[test]
fn a_second_showing_or_a_second_device_does_not_renew_the_clock() {
    let (store, clock) = timed();
    let id = ask(&store, "sha256:twice", 60);

    clock.advance_secs(10);
    let first = store.show_approval(&id).expect("first showing");
    clock.advance_secs(40);
    let second = store.show_approval(&id).expect("second showing, same or other device");

    assert_eq!(second.shown_at, first.shown_at, "the first showing is the one kept");
    assert_eq!(second.expires_at, first.expires_at, "the TTL is not renewed");
    // The first showing (T+10) ends the TTL at T+70, whatever the second one did at T+50.
    clock.advance_secs(19);
    assert_eq!(status(&store, &id), ApprovalStatus::Pending);
    clock.advance_secs(1);
    assert_eq!(status(&store, &id), ApprovalStatus::Expired);
}

#[test]
fn a_card_never_shown_stays_pending_past_any_ttl_and_waits_on_nothing_to_change() {
    let (store, clock) = timed();
    let id = ask(&store, "sha256:unseen", 1);
    clock.advance_secs(10_000);

    assert_eq!(status(&store, &id), ApprovalStatus::Pending);
    let waits = store.approval_waits(&["run_clock".to_string()]).expect("waits");
    assert_eq!(waits.pending, 1);
    assert_eq!(waits.next_change, None, "an unseen card has no TTL change to wait for");
}

#[test]
fn a_card_decided_before_it_is_shown_is_not_given_a_clock() {
    let (store, _clock) = timed();
    let id = ask(&store, "sha256:decided", 60);
    store
        .decide_approval(&id, Decision::Approve, "client-1", "device-1")
        .expect("approved");

    let late = store.show_approval(&id).expect("a late showing answers as it is");
    assert_eq!(late.status, ApprovalStatus::Approved);
    assert!(late.shown_at.is_none(), "a decided card does not start a clock");
}

#[test]
fn a_legacy_row_without_a_ttl_keeps_the_deadline_it_was_written_with() {
    let (store, clock) = timed();
    let id = ask(&store, "sha256:legacy", 60);
    // What an alpha.3 row looks like after the v29 migration: no TTL, no showing.
    store
        .lock()
        .execute(
            "UPDATE approval SET ttl_seconds = NULL, shown_at = NULL WHERE id = ?1",
            [&id],
        )
        .expect("legacy row");

    clock.advance_secs(61);
    assert_eq!(status(&store, &id), ApprovalStatus::Expired);
}

#[test]
fn the_wait_of_the_requester_is_not_bounded_by_the_ttl_before_the_card_is_shown() {
    // A requester asks for a decision with no deadline of its own: while the card
    // is unseen, the wait has no next change to sleep until.
    let (store, clock) = timed();
    let id = ask(&store, "sha256:requester", 1);
    clock.advance_secs(5);
    let waits = store.approval_waits(&["run_clock".to_string()]).expect("waits");
    assert_eq!(waits.next_change, None);
    assert_eq!(status(&store, &id), ApprovalStatus::Pending);
}
