//! Store-level acceptance tests: migrations, idempotency, pagination, search,
//! persistence across a reopen, and interrupted turns.

use std::path::PathBuf;

use hn_core_chat::{
    ChatStore, ConversationFilter, ConversationUpdate, NewConversation, RunState, SearchMode,
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

fn all(store: &ChatStore) -> Vec<hn_core_chat::Conversation> {
    store
        .list_conversations(ConversationFilter {
            q: None,
            section_id: None,
            pinned: false,
            archived: false,
            cursor: None,
            limit: 50,
        })
        .expect("list")
        .conversations
}

#[test]
fn schema_and_migrations_are_idempotent() {
    let dir = temp_dir("schema");
    let path = dir.join("chat.db");
    let (first_id, alfred_id, ernesto_id) = {
        let store = ChatStore::open(&path).expect("first open");
        let bootstrap = store.bootstrap().expect("bootstrap");
        assert_eq!(bootstrap.company.name, "Mi empresa");
        assert_eq!(bootstrap.alfred.name, "Alfred");
        assert!(bootstrap.projects.is_empty(), "no project is invented");
        let ernesto_id: String = {
            let connection = rusqlite::Connection::open(&path).expect("raw");
            connection
                .query_row(
                    "SELECT id FROM agent_seat WHERE role_id = 'lead'",
                    [],
                    |row| row.get(0),
                )
                .expect("ernesto")
        };
        assert!(ernesto_id.starts_with("ag_"));
        (bootstrap.company.id, bootstrap.alfred.agent_id, ernesto_id)
    };
    {
        let store = ChatStore::open(&path).expect("second open");
        let bootstrap = store.bootstrap().expect("bootstrap");
        assert_eq!(bootstrap.company.id, first_id);
        assert_eq!(bootstrap.alfred.agent_id, alfred_id);
        let connection = rusqlite::Connection::open(&path).expect("raw");
        let seats: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_seat", [], |row| row.get(0))
            .expect("seats");
        assert_eq!(seats, 2, "the seed is not duplicated");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 6, "the approval mode tables are schema 6");
        assert_eq!(
            connection
                .query_row(
                    "SELECT id FROM agent_seat WHERE role_id = 'lead'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .expect("lead"),
            ernesto_id
        );
    }
}

#[test]
fn client_id_is_idempotent() {
    let (store, _path, _dir) = store("idempotent");
    let id = conversation(&store, "Chat");
    let first = store
        .append_user_message(&id, "client-1", "hola")
        .expect("first");
    let revision = store.revision().expect("revision");
    let second = store
        .append_user_message(&id, "client-1", "hola")
        .expect("duplicate");
    assert!(!first.duplicate);
    assert!(second.duplicate);
    assert_eq!(first.message.id, second.message.id);
    assert_eq!(first.message.seq, second.message.seq);
    assert_eq!(first.turn.id, second.turn.id);
    assert_eq!(store.revision().expect("revision"), revision);
    let page = store.timeline(&id, None, 50).expect("timeline");
    assert_eq!(page.entries.len(), 1);
}

#[test]
fn timeline_paginates_by_seq() {
    let (store, _path, _dir) = store("timeline");
    let id = conversation(&store, "Chat");
    seed_agent_messages(
        &store,
        &id,
        &[
            "mensaje 1",
            "mensaje 2",
            "mensaje 3",
            "mensaje 4",
            "mensaje 5",
        ],
    );
    let latest = store.timeline(&id, None, 2).expect("first page");
    assert_eq!(
        latest
            .entries
            .iter()
            .map(|entry| entry.seq)
            .collect::<Vec<_>>(),
        vec![5, 6]
    );
    assert_eq!(latest.next_before_seq, Some(5));
    let middle = store
        .timeline(&id, latest.next_before_seq, 2)
        .expect("second page");
    assert_eq!(
        middle
            .entries
            .iter()
            .map(|entry| entry.seq)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    let oldest = store
        .timeline(&id, middle.next_before_seq, 2)
        .expect("third page");
    assert_eq!(
        oldest
            .entries
            .iter()
            .map(|entry| entry.seq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(oldest.next_before_seq, None);
}

#[test]
fn search_finds_title_and_message_bodies() {
    let (store, _path, _dir) = store("search");
    let id = conversation(&store, "Plan Demo");
    seed_agent_messages(&store, &id, &["el presupuesto anual"]);
    assert!(matches!(
        store.search_mode(),
        SearchMode::Fts5 | SearchMode::Like
    ));

    let by_title = store
        .list_conversations(ConversationFilter {
            q: Some("Demo".to_string()),
            section_id: None,
            pinned: false,
            archived: false,
            cursor: None,
            limit: 10,
        })
        .expect("search title");
    assert_eq!(by_title.conversations.len(), 1);

    let by_body = store
        .list_conversations(ConversationFilter {
            q: Some("presupuesto".to_string()),
            section_id: None,
            pinned: false,
            archived: false,
            cursor: None,
            limit: 10,
        })
        .expect("search body");
    assert_eq!(by_body.conversations.len(), 1);

    let missing = store
        .list_conversations(ConversationFilter {
            q: Some("inexistente".to_string()),
            section_id: None,
            pinned: false,
            archived: false,
            cursor: None,
            limit: 10,
        })
        .expect("search none");
    assert!(missing.conversations.is_empty());
}

#[test]
fn metadata_survives_a_reopen() {
    let (store, path, _dir) = store("metadata");
    let id = conversation(&store, "Chat");
    let section = store.upsert_section(None, "Trabajo").expect("section");
    let updated = store
        .update_conversation(
            &id,
            ConversationUpdate {
                title: Some("Plan semanal".to_string()),
                pinned: Some(true),
                section: SectionChange::Set(section.id.clone()),
                archived: None,
            },
        )
        .expect("update");
    assert_eq!(updated.title, "Plan semanal");
    assert!(updated.pinned);
    assert_eq!(updated.section_id.as_deref(), Some(section.id.as_str()));
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
    assert!(all(&store).is_empty(), "archived is not in Recientes");
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    let detail = reopened.conversation(&id).expect("detail");
    assert_eq!(detail.conversation.title, "Plan semanal");
    assert!(detail.conversation.pinned);
    assert!(detail.conversation.archived);
    assert_eq!(
        detail.conversation.section_id.as_deref(),
        Some(section.id.as_str())
    );
    let archived = reopened
        .list_conversations(ConversationFilter {
            q: None,
            section_id: Some(section.id),
            pinned: true,
            archived: true,
            cursor: None,
            limit: 10,
        })
        .expect("archived list");
    assert_eq!(archived.conversations.len(), 1);
}

#[test]
fn a_running_turn_is_interrupted_after_a_reopen() {
    let (store, path, _dir) = store("interrupted");
    let id = conversation(&store, "Chat");
    let accepted = store
        .append_user_message(&id, "client-1", "sigue trabajando")
        .expect("message");
    assert_eq!(accepted.turn.state, RunState::Queued);
    assert!(store.start_run(&accepted.turn.id).expect("start"));
    assert_eq!(
        store.run_state(&accepted.turn.id).expect("state"),
        Some(RunState::Running)
    );
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    assert_eq!(
        reopened.run_state(&accepted.turn.id).expect("state"),
        Some(RunState::Interrupted)
    );
    let page = reopened.timeline(&id, None, 10).expect("timeline");
    assert_eq!(page.entries.len(), 1, "history survives the restart");
    assert_eq!(page.entries[0].seq, 1);
}
