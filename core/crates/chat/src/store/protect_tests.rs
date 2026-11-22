//! `ChatStore::protect`: an open store moves its database to the keyed form and goes on writing.

use std::fs;

use excubia_persist::vault::{EntropyPool, VaultKey};
use excubia_test_dir::TestDir;

use crate::{ChatStore, NewConversation};

const PLAIN_HEADER: &[u8] = b"SQLite format 3\0";

fn fresh_key() -> VaultKey {
    VaultKey::generate(&EntropyPool::new()).expect("key")
}

fn titled(title: &str) -> NewConversation {
    NewConversation {
        title: Some(title.to_string()),
        project_id: None,
    }
}

#[test]
fn an_open_store_moves_to_the_keyed_file_and_keeps_writing() {
    let dir = TestDir::new("protect-live");
    let path = dir.path().join("chat.db");
    let store = ChatStore::open(&path).expect("store");
    let id = store
        .create_conversation(titled("Trabajo"))
        .expect("chat")
        .id;
    let key = fresh_key();

    store
        .protect(&key, &dir.path().join("backup"))
        .expect("protect");

    assert!(!fs::read(&path).expect("file").starts_with(PLAIN_HEADER));
    let title = |store: &ChatStore| store.conversation(&id).expect("chat").conversation.title;
    assert_eq!(title(&store), "Trabajo");
    store
        .create_conversation(titled("Nuevo"))
        .expect("the store still writes");
    drop(store);

    assert!(
        ChatStore::open(&path).is_err(),
        "a plain open must refuse the keyed file"
    );
    let reopened = ChatStore::open_with_key(&path, Some(&key)).expect("keyed reopen");
    assert_eq!(title(&reopened), "Trabajo");
}

#[test]
fn a_move_that_fails_keeps_the_plain_file_and_the_store_writes_on() {
    let dir = TestDir::new("protect-failed");
    let path = dir.path().join("chat.db");
    let store = ChatStore::open(&path).expect("store");
    store.create_conversation(titled("Trabajo")).expect("chat");
    // The backup folder would sit under a file, so it cannot be made.
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, b"not a folder").expect("blocker");

    store
        .protect(&fresh_key(), &blocker.join("backup"))
        .expect_err("the move must stop");

    assert!(fs::read(&path).expect("file").starts_with(PLAIN_HEADER));
    store
        .create_conversation(titled("Despues"))
        .expect("the store still writes");
}
