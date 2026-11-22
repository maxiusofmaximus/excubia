//! The conversations that were created before the title rules still say «Nuevo chat»:
//! opening the store titles them once, from their first message, and nothing else moves.

use excubia_chat::{ChatStore, NewConversation};
use excubia_test_dir::TestDir;

fn new_conversation(store: &ChatStore, title: Option<&str>) -> String {
    store
        .create_conversation(NewConversation {
            title: title.map(str::to_string),
            project_id: None,
        })
        .expect("conversation")
        .id
}

#[test]
fn a_legacy_untitled_conversation_is_titled_once_and_keeps_its_place() {
    let dir = TestDir::new("core-chat-tests-legacy-titles");
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    let untitled = new_conversation(&store, None);
    store
        .append_user_message(&untitled, "c-1", "Borra el archivo README.md", None)
        .expect("message");
    let empty = new_conversation(&store, None);
    let own = new_conversation(&store, Some("Mi título"));
    store
        .append_user_message(&own, "c-2", "Resume el correo del banco", None)
        .expect("message");

    // The rule of today only runs for new conversations: put the legacy title back.
    let raw = rusqlite::Connection::open(&path).expect("raw connection");
    raw.execute(
        "UPDATE conversation SET title = 'Nuevo chat' WHERE id = ?1",
        [&untitled],
    )
    .expect("legacy title");
    let before = store.conversation(&untitled).expect("detail").conversation;
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    let after = reopened.conversation(&untitled).expect("detail").conversation;
    assert_eq!(after.title, "Borrar README.md");
    assert_eq!(after.updated_at, before.updated_at, "the history must not reorder");
    assert_eq!(
        reopened.conversation(&empty).expect("detail").conversation.title,
        "Nuevo chat"
    );
    assert_eq!(
        reopened.conversation(&own).expect("detail").conversation.title,
        "Mi título"
    );

    // A second pass finds nothing left to title.
    assert_eq!(reopened.retitle_untitled().expect("second pass"), 0);
}
