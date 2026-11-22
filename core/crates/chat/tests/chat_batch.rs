//! A merged turn on the store side: messages that arrive during a running turn never
//! interrupt it, one reply closes every run of the batch, and a failure fails them all
//! with one card.

use excubia_chat::{ChatStore, NewConversation, RunState};

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Lote".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// A running first turn and three messages that wait behind it. Returns the run ids
/// of the three waiting messages, in the order they were sent.
fn running_with_three_waiting(store: &ChatStore, id: &str) -> (String, [String; 3]) {
    let first = store
        .append_user_message(id, "c-1", "uno", None)
        .expect("first");
    assert!(store.start_run(&first.turn.id).expect("start"));
    let mut waiting = Vec::new();
    for (client, text) in [("c-2", "dos"), ("c-3", "tres"), ("c-4", "cuatro")] {
        let accepted = store
            .append_user_message(id, client, text, None)
            .expect("waiting");
        waiting.push(accepted.turn.id);
    }
    let [a, b, c]: [String; 3] = waiting.try_into().expect("three");
    (first.turn.id, [a, b, c])
}

#[test]
fn a_message_that_arrives_during_a_running_turn_does_not_interrupt_it() {
    let store = ChatStore::in_memory().expect("store");
    let id = conversation(&store);
    let (running, _) = running_with_three_waiting(&store, &id);
    assert_eq!(
        store.run_state(&running).expect("state"),
        Some(RunState::Running)
    );
}

#[test]
fn one_reply_completes_every_run_of_a_merged_turn() {
    let store = ChatStore::in_memory().expect("store");
    let id = conversation(&store);
    let (running, [a, b, c]) = running_with_three_waiting(&store, &id);
    for run in [&a, &b, &c] {
        assert!(store.start_run(run).expect("start"));
    }

    let reply = store
        .complete_runs_with_files(&[&a, &b, &c], "Respuesta a los tres", &[], &[])
        .expect("reply");
    assert_eq!(reply.author_kind, "agent");
    for run in [&a, &b, &c] {
        assert_eq!(
            store.run_state(run).expect("state"),
            Some(RunState::Completed)
        );
    }
    assert_eq!(
        store.run_state(&running).expect("state"),
        Some(RunState::Running),
        "the turn before the batch is not part of it"
    );
    assert!(
        store
            .complete_runs_with_files(&[&a, &b, &c], "otra vez", &[], &[])
            .is_err(),
        "a batch answers once"
    );
}

#[test]
fn a_failed_merged_turn_fails_every_run_and_writes_one_card() {
    let store = ChatStore::in_memory().expect("store");
    let id = conversation(&store);
    let (_, [a, b, c]) = running_with_three_waiting(&store, &id);
    for run in [&a, &b, &c] {
        assert!(store.start_run(run).expect("start"));
    }

    assert!(store
        .fail_runs_with_card(&[&a, &b, &c], "process_exit: 1")
        .expect("fail"));
    for run in [&a, &b, &c] {
        assert_eq!(
            store.run_state(run).expect("state"),
            Some(RunState::Failed)
        );
    }
    let cards = store
        .timeline(&id, None, 50)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.state.as_deref() == Some("failed"))
        .count();
    assert_eq!(cards, 1, "one failed card for the whole batch");
}

#[test]
fn only_a_message_the_user_wrote_is_a_user_turn() {
    let store = ChatStore::in_memory().expect("store");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "c-1", "hola", None)
        .expect("message");
    assert!(store.is_user_turn(&accepted.message.id).expect("author"));
    assert!(!store.is_user_turn("msg_missing").expect("unknown"));
}
