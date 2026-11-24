//! `approval_waits`: the approvals a run waits on, the only signal the seat's turn
//! budget pauses on. Only pending approvals of the given runs count.

use std::time::Duration;

use excubia_test_dir::TestDir;
use serde_json::json;

use crate::model::{ConversationUpdate, SectionChange};
use crate::model_approval::{ApprovalKind, Decision, NewApproval, Risk};
use crate::{ChatStore, NewConversation};

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn ask(
    store: &ChatStore,
    run_id: &str,
    conversation_id: Option<&str>,
    digest: &str,
    ttl_seconds: u32,
) -> String {
    store
        .create_approval(NewApproval {
            conversation_id: conversation_id.map(str::to_string),
            seat_id: None,
            agent_label: None,
            run_id: Some(run_id.to_string()),
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

fn runs(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn only_the_pending_approvals_of_the_given_runs_count() {
    let store = ChatStore::in_memory().expect("store");
    ask(&store, "run_1", None, "sha256:a", 600);
    ask(&store, "run_2", None, "sha256:b", 600);
    let denied = ask(&store, "run_1", None, "sha256:c", 600);
    store
        .decide_approval(&denied, Decision::Deny, "device-1", "device-1")
        .expect("denied");

    let mine = store.approval_waits(&runs(&["run_1"])).expect("waits");
    assert_eq!(
        mine.pending, 1,
        "the denied card and the other run do not wait"
    );
    assert!(
        mine.next_change.is_some(),
        "a pending card has a next change"
    );

    let other = store.approval_waits(&runs(&["run_2"])).expect("waits");
    assert_eq!(other.pending, 1);
    let none = store.approval_waits(&runs(&["run_3"])).expect("waits");
    assert_eq!(none.pending, 0);
    assert_eq!(none.next_change, None);
}

#[test]
fn a_merged_turn_counts_the_approvals_of_every_run_it_answers() {
    let store = ChatStore::in_memory().expect("store");
    ask(&store, "run_2", None, "sha256:a", 600);
    let waits = store
        .approval_waits(&runs(&["run_1", "run_2"]))
        .expect("waits");
    assert_eq!(waits.pending, 1);
}

#[test]
fn an_archived_conversation_does_not_suspend_its_runs() {
    let store = ChatStore::in_memory().expect("store");
    let archived = conversation(&store);
    store
        .update_conversation(
            &archived,
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archived");
    ask(&store, "run_1", Some(&archived), "sha256:a", 600);
    let waits = store.approval_waits(&runs(&["run_1"])).expect("waits");
    assert_eq!(
        waits.pending, 0,
        "a card in an archived conversation is not waited on"
    );
}

#[test]
fn an_expired_approval_stops_counting_on_its_own() {
    let store = ChatStore::in_memory().expect("store");
    ask(&store, "run_1", None, "sha256:a", 1);
    let waiting = store.approval_waits(&runs(&["run_1"])).expect("waits");
    assert_eq!(waiting.pending, 1);
    let until = waiting.next_change.expect("the TTL is the next change");
    assert!(until <= Duration::from_secs(1), "{until:?}");

    std::thread::sleep(until + Duration::from_millis(200));
    let after = store.approval_waits(&runs(&["run_1"])).expect("waits");
    assert_eq!(
        after.pending, 0,
        "the TTL expired the card: nothing waits now"
    );
}

#[test]
fn a_restart_closes_the_waits_of_the_runs_it_interrupted() {
    let dir = TestDir::new("approval-waits-restart");
    let path = dir.path().join("chat.db");
    {
        let store = ChatStore::open(&path).expect("store");
        ask(&store, "run_1", None, "sha256:a", 600);
        assert_eq!(
            store
                .approval_waits(&runs(&["run_1"]))
                .expect("waits")
                .pending,
            1
        );
    }
    let reopened = ChatStore::open(&path).expect("reopened");
    let waits = reopened.approval_waits(&runs(&["run_1"])).expect("waits");
    assert_eq!(
        waits.pending, 0,
        "a card waiting at the last start is closed, not waited on"
    );
}
