//! Stopping a conversation's open turn: the store side. Nothing is deleted, the
//! user message journals `interrupted`, and a stop never reopens a closed run.

use std::path::PathBuf;

use hn_core_chat::{ChatError, ChatStore, NewConversation, RunState};
use hn_core_test_dir::TestDir;

fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("core-chat-stop-{name}"));
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
}

fn conversation(store: &ChatStore, title: &str) -> String {
    store
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn turn_state(store: &ChatStore, conversation_id: &str, message_id: &str) -> Option<String> {
    store
        .timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
        .into_iter()
        .find(|entry| entry.message_id.as_deref() == Some(message_id))
        .and_then(|entry| entry.turn_state)
}

/// The `updated` frames of one message that carry a turn state, oldest first.
fn turn_updates(store: &ChatStore, message_id: &str) -> Vec<String> {
    store
        .events_after(0, 1000)
        .expect("events")
        .iter()
        .filter(|frame| frame.body["message_id"] == message_id)
        .filter_map(|frame| frame.body["turn_state"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn stopping_a_running_turn_interrupts_it_and_journals_the_user_message() {
    let (store, _path, _dir) = store("running");
    let id = conversation(&store, "Parar");
    let accepted = store
        .append_user_message(&id, "stop-1", "hola", None)
        .expect("message");
    let message_id = &accepted.message.id;
    assert!(store.start_run(&accepted.turn.id).expect("start"));

    let stopped = store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop");
    assert_eq!(stopped, vec![accepted.turn.id.clone()]);
    assert_eq!(
        store.run_state(&accepted.turn.id).expect("state"),
        Some(RunState::Interrupted)
    );
    assert_eq!(
        turn_state(&store, &id, message_id).as_deref(),
        Some("interrupted")
    );
    assert_eq!(
        turn_updates(&store, message_id).last().map(String::as_str),
        Some("interrupted"),
        "the stop is journaled as the user message's last turn state"
    );

    let again = store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("second stop");
    assert!(again.is_empty(), "a second stop finds no open turn");
}

#[test]
fn stopping_a_queued_turn_keeps_it_from_ever_starting() {
    let (store, _path, _dir) = store("queued");
    let id = conversation(&store, "En cola");
    let accepted = store
        .append_user_message(&id, "stop-queued", "hola", None)
        .expect("message");

    let stopped = store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop");
    assert_eq!(stopped, vec![accepted.turn.id.clone()]);
    assert!(
        !store.start_run(&accepted.turn.id).expect("start"),
        "the worker skips a turn that is no longer queued"
    );
    assert_eq!(
        turn_state(&store, &id, &accepted.message.id).as_deref(),
        Some("interrupted")
    );
}

#[test]
fn a_stop_without_an_open_turn_changes_nothing() {
    let (store, _path, _dir) = store("idle");
    let id = conversation(&store, "Libre");
    let revision = store.revision().expect("revision");

    assert!(store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop")
        .is_empty());
    assert_eq!(store.revision().expect("revision"), revision);
}

#[test]
fn a_completed_turn_is_not_stopped_and_a_late_reply_cannot_close_a_stopped_one() {
    let (store, _path, _dir) = store("late");
    let id = conversation(&store, "Tarde");
    let done = store
        .append_user_message(&id, "stop-done", "uno", None)
        .expect("message");
    store.start_run(&done.turn.id).expect("start");
    store
        .complete_run_with_reply(&done.turn.id, "listo", &[])
        .expect("reply");

    assert!(store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop")
        .is_empty());
    assert_eq!(
        store.run_state(&done.turn.id).expect("state"),
        Some(RunState::Completed)
    );

    let open = store
        .append_user_message(&id, "stop-open", "dos", None)
        .expect("message");
    store.start_run(&open.turn.id).expect("start");
    store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop");
    let late = store.complete_run_with_reply(&open.turn.id, "tarde", &[]);
    assert!(matches!(late, Err(ChatError::RunNotActive(_))));
    assert_eq!(
        store.run_state(&open.turn.id).expect("state"),
        Some(RunState::Interrupted)
    );
}

#[test]
fn a_stop_touches_only_its_own_conversation() {
    let (store, _path, _dir) = store("scoped");
    let mine = conversation(&store, "Mía");
    let other = conversation(&store, "Otra");
    let kept = store
        .append_user_message(&other, "stop-other", "sigue", None)
        .expect("message");
    store.start_run(&kept.turn.id).expect("start");
    let stopped = store
        .append_user_message(&mine, "stop-mine", "para", None)
        .expect("message");
    store.start_run(&stopped.turn.id).expect("start");

    store
        .stop_conversation_runs(&mine, "stopped_by_user")
        .expect("stop");
    assert_eq!(
        store.run_state(&kept.turn.id).expect("state"),
        Some(RunState::Running)
    );
    assert_eq!(
        store.run_state(&stopped.turn.id).expect("state"),
        Some(RunState::Interrupted)
    );
}

#[test]
fn stopping_an_unknown_conversation_is_not_found() {
    let (store, _path, _dir) = store("unknown");
    assert!(matches!(
        store.stop_conversation_runs("cnv_missing", "stopped_by_user"),
        Err(ChatError::NoSuchConversation)
    ));
}
