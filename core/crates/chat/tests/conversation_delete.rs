//! Deleting a conversation for good (TASK-CONV-BORRAR): every row that names it
//! goes in one transaction, the notes keep their text without the link, the
//! audit names no title, and a turn, delegation or approval still open refuses it.

mod conversation_delete_support;

use conversation_delete_support::{
    conversation_folder, counts, new_conversation, populate, populate_other, second_connection,
    start, DEVICE,
};
use excubia_chat::{ChatError, ChatStore, ConversationFilter};

#[test]
fn a_delete_leaves_no_row_of_the_conversation_and_keeps_the_notes() {
    let fixture = start("full");
    let conversation = fixture.conversation.clone();
    let other = new_conversation(&fixture.store, "Otra");
    populate(&fixture, &conversation);
    populate_other(&fixture, &other);
    let connection = second_connection(&fixture.db);
    let before = counts(&connection, &conversation);
    let other_before = counts(&connection, &other);
    assert!(
        before.iter().all(|(_, rows)| *rows > 0),
        "every table is filled: {before:?}"
    );
    assert!(before.iter().any(|(table, _)| table == "conversation_fts"));
    assert!(conversation_folder(&fixture, &conversation).exists());

    fixture
        .store
        .delete_conversation(&conversation, DEVICE)
        .expect("delete");

    let after = counts(&connection, &conversation);
    assert!(after.iter().all(|(_, rows)| *rows == 0), "{after:?}");
    assert_eq!(counts(&connection, &other), other_before);
    let fts_hits: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM conversation_fts WHERE conversation_fts MATCH 'Presupuesto'",
            [],
            |row| row.get(0),
        )
        .expect("fts");
    assert_eq!(fts_hits, 0);
    assert!(!conversation_folder(&fixture, &conversation).exists());
    let mut check = connection
        .prepare("PRAGMA foreign_key_check")
        .expect("check");
    assert!(check
        .query([])
        .expect("rows")
        .next()
        .expect("next")
        .is_none());
    let note: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT conversation_id, message_id FROM note WHERE id = 'nt_1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the note is kept");
    assert_eq!(note, (None, None));
    let approval: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT conversation_id, delegation_id FROM approval WHERE id = 'apr_1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the resolved approval is kept without its links");
    assert_eq!(approval, (None, None));
}

#[test]
fn the_audit_names_the_device_and_not_the_title() {
    let fixture = start("audit");
    fixture
        .store
        .delete_conversation(&fixture.conversation, DEVICE)
        .expect("delete");
    let connection = second_connection(&fixture.db);
    let rows: Vec<(String, String, String, String, String)> = connection
        .prepare("SELECT object_kind, object_id, action, actor_kind, actor_id FROM agenda_audit")
        .expect("audit")
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("collect");
    let expected = (
        "conversation".to_string(),
        fixture.conversation.clone(),
        "delete".to_string(),
        "device".to_string(),
        DEVICE.to_string(),
    );
    assert_eq!(rows, vec![expected]);
}

#[test]
fn the_other_devices_learn_of_the_delete_through_the_journal() {
    let fixture = start("journal");
    let before = fixture.store.revision().expect("revision");
    fixture
        .store
        .delete_conversation(&fixture.conversation, DEVICE)
        .expect("delete");
    let frames = fixture.store.events_after(before, 50).expect("events");
    let last = frames.last().expect("the delete is journaled");
    assert_eq!(last.event, "chat.conversation");
    assert_eq!(last.body["change"], "deleted");
    assert_eq!(last.body["conversation_id"], fixture.conversation.as_str());
    let connection = second_connection(&fixture.db);
    let journal_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM chat_event WHERE conversation_id = ?1",
            [&fixture.conversation],
            |row| row.get(0),
        )
        .expect("journal");
    assert_eq!(journal_rows, 0);
}

/// A conversation with one thing still open is refused, and keeps every row.
#[test]
fn a_turn_a_delegation_or_an_approval_still_open_refuses_the_delete() {
    let open_turn = "UPDATE run SET state = 'running' WHERE id = 'run_1'";
    let open_delegation = "UPDATE delegation SET state = 'waiting' WHERE id = 'dlg_1'";
    let open_approval = "UPDATE approval SET status = 'pending' WHERE id = 'apr_1'";
    for (name, change) in [
        ("turn", open_turn),
        ("delegation", open_delegation),
        ("approval", open_approval),
    ] {
        let fixture = start(name);
        populate(&fixture, &fixture.conversation);
        let connection = second_connection(&fixture.db);
        connection.execute(change, []).expect("open the work");
        let before = counts(&connection, &fixture.conversation);

        let error = fixture
            .store
            .delete_conversation(&fixture.conversation, DEVICE)
            .expect_err("busy");

        assert!(
            matches!(error, ChatError::ConversationBusy),
            "{name}: {error:?}"
        );
        assert_eq!(counts(&connection, &fixture.conversation), before, "{name}");
        assert!(fixture.store.conversation(&fixture.conversation).is_ok());
    }
}

#[test]
fn an_unknown_conversation_is_not_found_twice() {
    let fixture = start("missing");
    let conversation = fixture.conversation.clone();
    fixture
        .store
        .delete_conversation(&conversation, DEVICE)
        .expect("first delete");
    assert!(matches!(
        fixture.store.delete_conversation(&conversation, DEVICE),
        Err(ChatError::NoSuchConversation)
    ));
    assert!(matches!(
        fixture.store.delete_conversation("cnv_missing", DEVICE),
        Err(ChatError::NoSuchConversation)
    ));
}

#[test]
fn a_deleted_conversation_does_not_come_back_after_a_reopen() {
    let fixture = start("reopen");
    let conversation = fixture.conversation.clone();
    fixture
        .store
        .delete_conversation(&conversation, DEVICE)
        .expect("delete");
    let reopened = ChatStore::open(&fixture.db).expect("reopen");
    assert!(matches!(
        reopened.conversation(&conversation),
        Err(ChatError::NoSuchConversation)
    ));
    let filter = ConversationFilter {
        q: None,
        section_id: None,
        pinned: false,
        archived: false,
        attention: false,
        working: false,
        cursor: None,
        limit: 30,
    };
    let page = reopened.list_conversations(filter).expect("list");
    assert!(page.conversations.iter().all(|row| row.id != conversation));
}
