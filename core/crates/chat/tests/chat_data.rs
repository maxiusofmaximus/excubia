//! Chat data for the quick replies, the quotes and the turn state: what the
//! timeline and the conversation list carry, and what each transition journals.

use std::path::PathBuf;

use excubia_chat::{
    action_digest, ApprovalKind, ChatError, ChatStore, Conversation, ConversationFilter, Decision,
    NewApproval, NewConversation, RunState, TimelineEntry, DEFAULT_TTL_SECONDS,
};
use excubia_test_dir::TestDir;
use serde_json::json;

fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("core-chat-data-{name}"));
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Chat".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn timeline(store: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    store
        .timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
}

fn entry<'a>(entries: &'a [TimelineEntry], message_id: &str) -> &'a TimelineEntry {
    entries
        .iter()
        .find(|entry| entry.message_id.as_deref() == Some(message_id))
        .expect("entry")
}

fn turn_state(store: &ChatStore, conversation_id: &str, message_id: &str) -> Option<String> {
    entry(&timeline(store, conversation_id), message_id)
        .turn_state
        .clone()
}

/// The `turn_state` of each `updated` frame of one message, oldest first.
fn turn_updates(store: &ChatStore, message_id: &str) -> Vec<String> {
    store
        .events_after(0, 1000)
        .expect("events")
        .iter()
        .filter(|frame| frame.body["message_id"] == message_id)
        .filter(|frame| frame.body["change"] == "updated")
        .filter_map(|frame| frame.body["turn_state"].as_str().map(str::to_owned))
        .collect()
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

fn keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn a_turn_moves_through_queued_running_and_completed_and_each_step_is_journaled() {
    let (store, _path, _dir) = store("turn-completed");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    let message_id = &accepted.message.id;
    assert_eq!(
        turn_state(&store, &id, message_id).as_deref(),
        Some("queued")
    );

    assert!(store.start_run(&accepted.turn.id).expect("start"));
    assert!(
        !store.start_run(&accepted.turn.id).expect("second start"),
        "a dequeued turn is not dequeued twice"
    );
    assert_eq!(
        turn_state(&store, &id, message_id).as_deref(),
        Some("running")
    );

    store
        .complete_run_with_reply(&accepted.turn.id, "listo", &[])
        .expect("reply");
    assert_eq!(
        turn_state(&store, &id, message_id).as_deref(),
        Some("completed")
    );
    assert_eq!(
        turn_updates(&store, message_id),
        vec!["running", "completed"],
        "the queued state is the `created` frame; each later step is an `updated`"
    );
}

#[test]
fn a_failed_turn_and_an_interrupted_turn_are_journaled() {
    let (store, _path, _dir) = store("turn-failed");
    let id = conversation(&store);

    let failed = store
        .append_user_message(&id, "client-1", "uno", None)
        .expect("first");
    store.start_run(&failed.turn.id).expect("start");
    store
        .fail_run_with_card(&failed.turn.id, "agent_exited")
        .expect("fail");
    assert_eq!(
        turn_state(&store, &id, &failed.message.id).as_deref(),
        Some("failed")
    );
    assert_eq!(
        turn_updates(&store, &failed.message.id),
        vec!["running", "failed"]
    );

    let interrupted = store
        .append_user_message(&id, "client-2", "dos", None)
        .expect("second");
    store.start_run(&interrupted.turn.id).expect("start");
    assert_eq!(
        store.interrupt_active("host_shutdown").expect("shutdown"),
        1
    );
    assert_eq!(
        turn_state(&store, &id, &interrupted.message.id).as_deref(),
        Some("interrupted")
    );
    assert_eq!(
        turn_updates(&store, &interrupted.message.id),
        vec!["running", "interrupted"]
    );
}

#[test]
fn a_turn_open_at_restart_is_journaled_as_interrupted() {
    let (store, path, _dir) = store("turn-restart");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    store.start_run(&accepted.turn.id).expect("start");
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    assert_eq!(
        turn_state(&reopened, &id, &accepted.message.id).as_deref(),
        Some("interrupted")
    );
    assert_eq!(
        turn_updates(&reopened, &accepted.message.id),
        vec!["running", "interrupted"]
    );
}

#[test]
fn a_quote_is_read_from_the_message_it_answers_and_disappears_with_it() {
    let (store, _path, _dir) = store("quote");
    let id = conversation(&store);
    let question = store
        .append_user_message(&id, "client-1", "primera\n  pregunta   con espacios", None)
        .expect("question");
    let long = store
        .append_user_message(&id, "client-2", &"b".repeat(200), None)
        .expect("long");
    let reply = store
        .append_user_message(
            &id,
            "client-3",
            "respuesta",
            Some(("message", question.message.id.as_str())),
        )
        .expect("reply");
    let long_reply = store
        .append_user_message(
            &id,
            "client-4",
            "otra",
            Some(("message", long.message.id.as_str())),
        )
        .expect("long reply");

    let entries = timeline(&store, &id);
    let quote = entry(&entries, &reply.message.id)
        .quote
        .clone()
        .expect("quote");
    assert_eq!(quote.kind, "message");
    assert_eq!(quote.id, question.message.id);
    assert_eq!(quote.author_kind, "user");
    assert_eq!(quote.preview, "primera pregunta con espacios");
    let long_quote = entry(&entries, &long_reply.message.id)
        .quote
        .clone()
        .expect("long quote");
    assert_eq!(long_quote.preview.chars().count(), 120);
    assert!(long_quote.preview.ends_with('…'));

    store
        .withdraw_turn(&question.message.id)
        .expect("withdraw the original");
    let entries = timeline(&store, &id);
    assert!(
        entry(&entries, &reply.message.id).quote.is_none(),
        "a missing original leaves no quote"
    );
}

#[test]
fn a_reply_must_name_a_message_of_the_same_conversation() {
    let (store, _path, _dir) = store("reply-target");
    let id = conversation(&store);
    let other = conversation(&store);
    let elsewhere = store
        .append_user_message(&other, "client-1", "ajena", None)
        .expect("other conversation");
    let here = store
        .append_user_message(&id, "client-2", "propia", None)
        .expect("this conversation");

    for target in [
        ("message", "msg_missing"),
        ("message", elsewhere.message.id.as_str()),
    ] {
        let error = store
            .append_user_message(&id, "client-3", "x", Some(target))
            .expect_err("refused");
        assert!(matches!(error, ChatError::InvalidReplyTo(_)), "{error}");
    }
    let error = store
        .append_user_message(
            &id,
            "client-4",
            "x",
            Some(("card", here.message.id.as_str())),
        )
        .expect_err("cards are not replyable yet");
    assert!(matches!(error, ChatError::InvalidReplyTo(_)), "{error}");
}

#[test]
fn suggestions_round_trip_and_an_invalid_list_is_refused_without_closing_the_turn() {
    let (store, _path, _dir) = store("suggestions");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    store.start_run(&accepted.turn.id).expect("start");
    let valid = vec![
        "Sí".to_string(),
        "No".to_string(),
        "Más tarde".to_string(),
        "a".repeat(60),
    ];
    let reply = store
        .complete_run_with_reply(&accepted.turn.id, "¿Continúo?", &valid)
        .expect("reply");

    let entries = timeline(&store, &id);
    let agent = entry(&entries, &reply.id);
    assert_eq!(agent.suggestions, valid);
    let wire = serde_json::to_value(agent).expect("json");
    assert_eq!(wire["suggestions"], json!(valid));

    let next = store
        .append_user_message(&id, "client-2", "otra", None)
        .expect("second");
    store.start_run(&next.turn.id).expect("start");
    let invalid: [Vec<String>; 4] = [
        (1..=5).map(|n| format!("opción {n}")).collect(),
        vec![String::new()],
        vec!["a".repeat(61)],
        vec!["a\nb".to_string()],
    ];
    for batch in invalid {
        let error = store
            .complete_run_with_reply(&next.turn.id, "no", &batch)
            .expect_err("refused");
        assert!(matches!(error, ChatError::InvalidSuggestions(_)), "{error}");
    }
    assert_eq!(
        store.run_state(&next.turn.id).expect("state"),
        Some(RunState::Running),
        "a refused reply leaves the turn open"
    );
    store
        .complete_run_with_reply(&next.turn.id, "sí", &[])
        .expect("the turn still closes");
}

/// The clock of the store has milliseconds: a step waits one so the order is strict.
fn tick() {
    std::thread::sleep(std::time::Duration::from_millis(5));
}

/// The operator denies one approval of the conversation.
fn operator_denies(store: &ChatStore, conversation_id: &str, action: &str) {
    let args = json!({ "command": action });
    let approval = store
        .create_approval(NewApproval {
            conversation_id: Some(conversation_id.to_string()),
            seat_id: None,
            agent_label: Some("bob".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {action}"),
            cwd: Some("/work".to_string()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, "/work", &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval");
    store
        .decide_approval(&approval.id, Decision::Deny, "device-1", "device-1")
        .expect("deny");
}

#[test]
fn a_denial_after_the_last_user_message_drops_the_quick_replies() {
    let (store, _path, _dir) = store("chips-denied");
    let id = conversation(&store);
    let turn = store
        .append_user_message(&id, "client-1", "bórralo", None)
        .expect("message");
    store.start_run(&turn.turn.id).expect("start");
    tick();
    operator_denies(&store, &id, "rm -rf");
    tick();
    let chips = vec!["Sí, bórralo igualmente".to_string(), "No".to_string()];
    let reply = store
        .complete_run_with_reply(&turn.turn.id, "No lo borro.", &chips)
        .expect("reply");
    let entries = timeline(&store, &id);
    assert!(entry(&entries, &reply.id).suggestions.is_empty());
}

#[test]
fn a_reply_keeps_its_quick_replies_without_a_denial_or_with_an_older_one() {
    let (store, _path, _dir) = store("chips-kept");
    let id = conversation(&store);
    let chips = vec!["Sí".to_string(), "No".to_string()];

    let plain = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    store.start_run(&plain.turn.id).expect("start");
    let reply = store
        .complete_run_with_reply(&plain.turn.id, "¿Continúo?", &chips)
        .expect("reply");
    assert_eq!(entry(&timeline(&store, &id), &reply.id).suggestions, chips);

    // A denial before the user's next message is history: the next reply keeps its chips.
    let denied = store
        .append_user_message(&id, "client-2", "borra el archivo", None)
        .expect("message");
    store.start_run(&denied.turn.id).expect("start");
    tick();
    operator_denies(&store, &id, "rm a.txt");
    tick();
    let next = store
        .append_user_message(&id, "client-3", "gracias", None)
        .expect("message");
    store.start_run(&next.turn.id).expect("start");
    let reply = store
        .complete_run_with_reply(&next.turn.id, "De nada.", &chips)
        .expect("reply");
    assert_eq!(entry(&timeline(&store, &id), &reply.id).suggestions, chips);
}

#[test]
fn active_runs_counts_the_open_turns_and_is_omitted_at_zero() {
    let (store, _path, _dir) = store("active-runs");
    let id = conversation(&store);
    let wire = serde_json::to_value(listed(&store, &id)).expect("json");
    assert!(wire.get("active_runs").is_none(), "no open turn: omitted");

    let first = store
        .append_user_message(&id, "client-1", "uno", None)
        .expect("first");
    let second = store
        .append_user_message(&id, "client-2", "dos", None)
        .expect("second");
    assert_eq!(listed(&store, &id).active_runs, 2);
    store.start_run(&first.turn.id).expect("start");
    assert_eq!(listed(&store, &id).active_runs, 2, "running counts as open");
    store
        .complete_run_with_reply(&first.turn.id, "ok", &[])
        .expect("reply");
    assert_eq!(listed(&store, &id).active_runs, 1);
    store
        .fail_run(&second.turn.id, "agent_busy")
        .expect("refuse");
    assert_eq!(listed(&store, &id).active_runs, 0);
    let wire = serde_json::to_value(listed(&store, &id)).expect("json");
    assert!(wire.get("active_runs").is_none());
}

#[test]
fn messages_without_the_new_fields_keep_their_old_shape() {
    let (store, path, _dir) = store("old-shape");
    let id = conversation(&store);
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    let reply = store
        .complete_run_with_reply(&accepted.turn.id, "listo", &[])
        .expect("reply");
    // A row from before version 11 has no turn recorded for its user message.
    rusqlite::Connection::open(&path)
        .expect("raw")
        .execute("DELETE FROM run", [])
        .expect("forget the turns");

    let entries = timeline(&store, &id);
    let user = serde_json::to_value(entry(&entries, &accepted.message.id)).expect("json");
    assert_eq!(
        keys(&user),
        [
            "author_kind",
            "created_at",
            "kind",
            "message_id",
            "preview",
            "seq",
            "state"
        ]
    );
    let agent = serde_json::to_value(entry(&entries, &reply.id)).expect("json");
    assert_eq!(
        keys(&agent),
        [
            "agent_id",
            "author_agent_id",
            "author_kind",
            "created_at",
            "kind",
            "message_id",
            "preview",
            "seq",
            "state",
        ]
    );
}

#[test]
fn a_version_ten_database_gains_the_reply_columns_and_keeps_its_messages() {
    let (store, path, _dir) = store("v10");
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
                "ALTER TABLE message DROP COLUMN suggestions_json;
                 ALTER TABLE message DROP COLUMN reply_to_kind;
                 ALTER TABLE message DROP COLUMN reply_to_id;
                 ALTER TABLE run DROP COLUMN progress;
                 ALTER TABLE run DROP COLUMN progress_at;
                 DROP TABLE attachment;
                 PRAGMA user_version = 10;",
            )
            .expect("a version ten database");
    }

    let reopened = ChatStore::open(&path).expect("migrated");
    let entries = timeline(&reopened, &id);
    assert_eq!(entries.len(), 2, "both messages survive the migration");
    assert_eq!(
        turn_state(&reopened, &id, &accepted.message.id).as_deref(),
        Some("completed")
    );
    let version: i64 = rusqlite::Connection::open(&path)
        .expect("raw")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 32);
}
