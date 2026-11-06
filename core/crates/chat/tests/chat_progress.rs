//! The progress line of a turn, store side: the window that holds back a line,
//! the frame that carries it, the clearing when the run closes, the field the
//! conversation shows, and the version 12 migration.

use std::path::PathBuf;
use std::sync::Arc;

use hn_core_chat::{
    ChatError, ChatStore, Conversation, ConversationFilter, ManualClock, NewConversation,
    ProgressOutcome, RunState,
};
use hn_core_test_dir::TestDir;
use serde_json::{json, Value};

fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("core-chat-progress-{name}"));
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Progreso".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// A conversation with one turn that is running and has a progress line.
fn running_with_a_line(store: &ChatStore) -> (String, String) {
    let id = conversation(store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    assert!(store.start_run(&accepted.turn.id).expect("start"));
    store
        .set_run_progress(&accepted.turn.id, "Revisando los borradores")
        .expect("line");
    (id, accepted.turn.id)
}

fn listed(store: &ChatStore, conversation_id: &str) -> Conversation {
    store
        .list_conversations(ConversationFilter {
            q: None,
            section_id: None,
            pinned: false,
            archived: false,
            attention: false,
            working: false,
            cursor: None,
            limit: 50,
        })
        .expect("list")
        .conversations
        .into_iter()
        .find(|conversation| conversation.id == conversation_id)
        .expect("listed")
}

/// The `text` of every `chat.progress` frame of one run, oldest first.
fn progress_frames(store: &ChatStore, run_id: &str) -> Vec<Value> {
    store
        .events_after(0, 1000)
        .expect("events")
        .into_iter()
        .filter(|frame| frame.event == "chat.progress" && frame.body["run_id"] == run_id)
        .map(|frame| frame.body["text"].clone())
        .collect()
}

#[test]
fn a_line_inside_the_window_is_held_back_and_the_stored_line_stays() {
    let (store, _path, _dir) = store("window");
    let clock = Arc::new(ManualClock::at(1_700_000_000_000));
    store.set_clock(clock.clone());
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    let run = &accepted.turn.id;
    assert!(store.start_run(run).expect("start"));

    assert_eq!(
        store
            .set_run_progress(run, "Revisando los borradores")
            .expect("first"),
        ProgressOutcome::Accepted
    );
    clock.advance_secs(2);
    assert_eq!(
        store.set_run_progress(run, "Otra línea").expect("held"),
        ProgressOutcome::Throttled
    );
    assert_eq!(
        listed(&store, &id).progress.as_deref(),
        Some("Revisando los borradores"),
        "the held line does not replace the stored one"
    );
    assert_eq!(
        progress_frames(&store, run),
        vec![json!("Revisando los borradores")],
        "a held line journals nothing"
    );

    clock.advance_secs(3);
    assert_eq!(
        store
            .set_run_progress(run, "Ahora sí")
            .expect("after the window"),
        ProgressOutcome::Accepted
    );
    assert_eq!(listed(&store, &id).progress.as_deref(), Some("Ahora sí"));
}

#[test]
fn a_closed_run_refuses_a_progress_line() {
    let (store, _path, _dir) = store("closed");
    let (_id, run) = running_with_a_line(&store);
    store
        .complete_run_with_reply(&run, "listo", &[])
        .expect("reply");

    assert!(matches!(
        store.set_run_progress(&run, "tarde"),
        Err(ChatError::RunNotActive(_))
    ));
}

#[test]
fn the_conversation_shows_the_line_only_while_its_turn_runs() {
    let (store, _path, _dir) = store("listing");
    let (id, run) = running_with_a_line(&store);
    let wire = serde_json::to_value(listed(&store, &id)).expect("json");
    assert_eq!(wire["progress"], "Revisando los borradores");

    store
        .complete_run_with_reply(&run, "listo", &[])
        .expect("reply");
    let wire = serde_json::to_value(listed(&store, &id)).expect("json");
    assert!(wire.get("progress").is_none(), "no open turn: omitted");
}

#[test]
fn a_completed_turn_clears_its_line_and_journals_the_clear() {
    let (store, _path, _dir) = store("completed");
    let (_id, run) = running_with_a_line(&store);
    store
        .complete_run_with_reply(&run, "listo", &[])
        .expect("reply");

    assert_eq!(
        progress_frames(&store, &run),
        vec![json!("Revisando los borradores"), Value::Null]
    );
}

#[test]
fn a_failed_turn_clears_its_line_and_journals_the_clear() {
    let (store, _path, _dir) = store("failed");
    let (_id, run) = running_with_a_line(&store);
    assert!(store
        .fail_run_with_card(&run, "process_exit: 1")
        .expect("fail"));

    assert_eq!(progress_frames(&store, &run).last(), Some(&Value::Null));
}

#[test]
fn an_interrupted_turn_clears_its_line_and_journals_the_clear() {
    let (store, _path, _dir) = store("interrupted");
    let (_id, run) = running_with_a_line(&store);
    store.interrupt_active("host_shutdown").expect("interrupt");

    assert_eq!(progress_frames(&store, &run).last(), Some(&Value::Null));
    assert_eq!(
        store.run_state(&run).expect("state"),
        Some(RunState::Interrupted)
    );
}

#[test]
fn a_stopped_turn_clears_its_line_and_journals_the_clear() {
    let (store, _path, _dir) = store("stopped");
    let (id, run) = running_with_a_line(&store);
    store
        .stop_conversation_runs(&id, "stopped_by_user")
        .expect("stop");

    assert_eq!(progress_frames(&store, &run).last(), Some(&Value::Null));
    assert!(listed(&store, &id).progress.is_none());
}

#[test]
fn a_run_open_at_restart_loses_its_line() {
    let (store, path, _dir) = store("restart");
    let (id, _run) = running_with_a_line(&store);
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    assert!(listed(&reopened, &id).progress.is_none());
}

#[test]
fn a_version_eleven_database_gains_the_progress_columns_and_keeps_its_runs() {
    let (store, path, _dir) = store("v11");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    store
        .complete_run_with_reply(&accepted.turn.id, "listo", &[])
        .expect("reply");
    drop(store);
    {
        let connection = rusqlite::Connection::open(&path).expect("raw");
        connection
            .execute_batch(
                "ALTER TABLE run DROP COLUMN progress;
                 ALTER TABLE run DROP COLUMN progress_at;
                 DROP TABLE attachment;
                 PRAGMA user_version = 11;",
            )
            .expect("a version eleven database");
    }

    let reopened = ChatStore::open(&path).expect("migrated");
    assert_eq!(
        reopened.run_state(&accepted.turn.id).expect("state"),
        Some(RunState::Completed)
    );
    let version: i64 = rusqlite::Connection::open(&path)
        .expect("raw")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 25);
}
