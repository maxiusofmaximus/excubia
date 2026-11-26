//! What a message page tells the shell about a turn: the reason of a failed card and the
//! model that wrote a reply. Both come from the run, so the page reads them from the store.

use std::path::PathBuf;

use excubia_chat::{ChatStore, Message, NewConversation};
use excubia_test_dir::TestDir;

fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("core-chat-message-meta-{name}"));
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
}

/// A conversation with one user message and its turn, started and ready to close.
fn started_turn(store: &ChatStore) -> (String, String) {
    let id = store
        .create_conversation(NewConversation {
            title: Some("Modelo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let accepted = store
        .append_user_message(&id, "client-1", "hola", None)
        .expect("message");
    assert!(store.start_run(&accepted.turn.id).expect("start"));
    (id, accepted.turn.id)
}

/// The agent messages of a conversation, as the shell reads them from a page.
fn agent_messages(store: &ChatStore, conversation_id: &str) -> Vec<Message> {
    store
        .messages(conversation_id, None, None, 50)
        .expect("page")
        .messages
        .into_iter()
        .filter(|message| message.author_kind == "agent")
        .collect()
}

#[test]
fn a_failed_card_carries_the_reason_of_its_run() {
    let (store, _path, _dir) = store("reason");
    let (id, run) = started_turn(&store);
    let reason = "free_models_exhausted; última salida: Free usage limit reached";
    assert!(store.fail_run_with_card(&run, reason).expect("fail"));

    let cards = agent_messages(&store, &id);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].state, "failed");
    assert_eq!(cards[0].failure_reason.as_deref(), Some(reason));
    assert_eq!(cards[0].model, None);
}

#[test]
fn a_reply_names_the_model_its_run_was_set_to() {
    let (store, _path, _dir) = store("model");
    let (id, run) = started_turn(&store);
    store
        .set_run_model(&run, "opencode/nemotron-3.5-lightning-free")
        .expect("model");
    store
        .complete_run_with_reply(&run, "listo", &[])
        .expect("reply");

    let replies = agent_messages(&store, &id);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].state, "complete");
    assert_eq!(
        replies[0].model.as_deref(),
        Some("opencode/nemotron-3.5-lightning-free")
    );
    assert_eq!(replies[0].failure_reason, None);
}

#[test]
fn a_reply_of_a_run_without_a_model_says_nothing_about_one() {
    let (store, _path, _dir) = store("no-model");
    let (id, run) = started_turn(&store);
    store
        .complete_run_with_reply(&run, "listo", &[])
        .expect("reply");

    let replies = agent_messages(&store, &id);
    assert_eq!(replies[0].model, None);
}
