//! Store-level acceptance tests: read cursors, the event journal, sections,
//! archives, cursor pagination, finished turns and the context window.

use std::path::PathBuf;

use hn_core_chat::{
    ChatError, ChatStore, ConversationFilter, ConversationUpdate, NewConversation, RunState,
    SectionChange,
};
use hn_core_test_dir::TestDir;

fn temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("core-chat-tests-{name}"))
}

/// The store, its file and the scratch directory that holds both.
fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = temp_dir(name);
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

/// Seed a conversation with agent-authored messages without inventing a second
/// code path: the user message opens a run and `ask_user` appends to its
/// timeline while the run is still active.
fn seed_agent_messages(store: &ChatStore, id: &str, texts: &[&str]) {
    let accepted = store
        .append_user_message(id, "seed-messages", "inicio")
        .expect("seed message");
    for text in texts {
        store
            .append_agent_question(&accepted.turn.id, text)
            .expect("agent message");
    }
}

#[test]
fn read_cursor_is_monotonic_and_clamped() {
    let (store, _path, _dir) = store("read");
    let id = conversation(&store, "Chat");
    seed_agent_messages(&store, &id, &["uno", "dos"]);
    assert_eq!(store.bootstrap().expect("bootstrap").unread.messages, 2);
    assert_eq!(store.mark_read(&id, 2).expect("read 2"), 2);
    assert_eq!(store.bootstrap().expect("bootstrap").unread.messages, 1);
    assert_eq!(store.mark_read(&id, 99).expect("clamp"), 3);
    assert_eq!(store.mark_read(&id, 0).expect("never backwards"), 3);
    assert_eq!(store.bootstrap().expect("bootstrap").unread.messages, 0);
}

#[test]
fn the_event_journal_replays_after_an_id() {
    let (store, _path, _dir) = store("events");
    let id = conversation(&store, "Chat");
    store
        .append_user_message(&id, "client-1", "hola")
        .expect("message");
    let all = store.events_after(0, 10).expect("all");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].event, "chat.conversation");
    assert_eq!(all[1].event, "chat.message");
    assert_eq!(all[1].body["preview"], "hola");
    let later = store.events_after(all[0].id, 10).expect("later");
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].id, all[1].id);
    assert!(store.events_after(all[1].id, 10).expect("none").is_empty());
    assert_eq!(store.revision().expect("revision"), all[1].id);
}

#[test]
fn sections_create_and_rename_in_order() {
    let (store, _path, _dir) = store("sections");
    let first = store.upsert_section(None, "Trabajo").expect("first");
    let second = store.upsert_section(None, "Personal").expect("second");
    assert!(first.order < second.order);
    let renamed = store
        .upsert_section(Some(&first.id), "Trabajo profundo")
        .expect("rename");
    assert_eq!(renamed.name, "Trabajo profundo");
    assert_eq!(renamed.order, first.order);
    let sections = store.sections().expect("sections");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "Trabajo profundo");
    assert!(matches!(
        store.upsert_section(Some("sec_nope"), "x"),
        Err(ChatError::UnknownSection(_))
    ));
}

#[test]
fn archived_conversations_reject_writes() {
    let (store, _path, _dir) = store("archived");
    let id = conversation(&store, "Chat");
    store
        .update_conversation(
            &id,
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archive");
    assert!(matches!(
        store.append_user_message(&id, "client-1", "hola"),
        Err(ChatError::ConversationArchived)
    ));
}

#[test]
fn cursor_pagination_does_not_repeat_rows() {
    let (store, _path, _dir) = store("cursor");
    for title in ["uno", "dos", "tres"] {
        conversation(&store, title);
    }
    let first = store
        .list_conversations(ConversationFilter {
            q: None,
            section_id: None,
            pinned: false,
            archived: false,
            cursor: None,
            limit: 2,
        })
        .expect("first");
    assert_eq!(first.conversations.len(), 2);
    let cursor = first.next_cursor.expect("cursor");
    let second = store
        .list_conversations(ConversationFilter {
            q: None,
            section_id: None,
            pinned: false,
            archived: false,
            cursor: Some(cursor),
            limit: 2,
        })
        .expect("second");
    assert_eq!(second.conversations.len(), 1);
    assert!(second
        .conversations
        .iter()
        .all(|row| first.conversations.iter().all(|first| first.id != row.id)));
}

#[test]
fn finishing_a_turn_records_its_end() {
    let (store, _path, _dir) = store("runs");
    let id = conversation(&store, "Chat");
    let accepted = store
        .append_user_message(&id, "client-1", "hola")
        .expect("message");
    assert!(store.start_run(&accepted.turn.id).expect("start"));
    store.fail_run(&accepted.turn.id, "test").expect("finish");
    assert_eq!(
        store.run_state(&accepted.turn.id).expect("state"),
        Some(RunState::Failed)
    );
}

#[test]
fn context_keeps_only_the_last_twenty_messages() {
    let (store, _path, _dir) = store("context-messages");
    let id = conversation(&store, "Chat");
    let texts: Vec<String> = (1..=25).map(|index| format!("m{index}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    seed_agent_messages(&store, &id, &refs);

    let context = store.context(&id, 20, 24_000).expect("context");
    assert_eq!(context.messages.len(), 20);
    assert_eq!(context.messages[0].seq, 7);
    assert!(context.truncated);
}

#[test]
fn context_keeps_only_the_last_twenty_four_thousand_chars() {
    let (store, _path, _dir) = store("context-chars");
    let id = conversation(&store, "Chat");
    let long = "x".repeat(1_000);
    seed_agent_messages(&store, &id, &[&long, &long, &long, &long]);

    let context = store.context(&id, 20, 2_500).expect("context");
    assert_eq!(context.messages.len(), 2);
    assert_eq!(context.messages[0].seq, 4);
    assert!(context.truncated);
}
