//! The owner leaves messages out of a batch (F2C1) over a scripted Microsoft and the
//! real chat store: the excluded message is skipped and never asked of Graph, the rest
//! moves in order, and the digest the batch was asked with still holds.

mod mail_move_support;
mod mail_support;

use std::sync::Arc;

use excubia_core_chat::{ApprovalStatus, ChatStore, Decision};
use excubia_core_seat::MoveFolder;
use excubia_core_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;

#[tokio::test]
async fn an_excluded_message_is_skipped_and_the_rest_of_the_batch_moves() {
    let dir = TestDir::new("mail-move-exclude");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            read("m2"),
            read("m3"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            moved_from("INBOX-ID", "NEW-m3"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(
        &tools,
        &[
            ("m1", MoveFolder::Junk),
            ("m2", MoveFolder::Junk),
            ("m3", MoveFolder::Deleted),
        ],
    )
    .await;
    let asked = chat.approval(&id).expect("approval").payload_redacted;
    assert_eq!(asked.expect("payload")["items"][1].get("excluded"), None);

    chat.decide_approval_excluding(
        &id,
        Decision::Approve,
        &["m2".to_owned()],
        "device-1",
        "device-1",
    )
    .expect("decide without m2");

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");

    let results = &reply.result["results"];
    assert_eq!(reply.result["status"], "applied");
    assert_eq!(results[0]["new_message_id"], "NEW-m1");
    assert_eq!(results[1]["skipped"], "owner");
    assert_eq!(results[1]["message_id"], "m2");
    assert!(results[1].get("new_message_id").is_none());
    assert_eq!(results[2]["new_message_id"], "NEW-m3");
    let posted = transport.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert!(posted[0].ends_with("/me/messages/m1/move"), "{posted:?}");
    assert!(posted[1].ends_with("/me/messages/m3/move"), "{posted:?}");
    assert_eq!(
        action_log(&dir)
            .list_recent(ACCOUNT, 10)
            .expect("log")
            .len(),
        2
    );

    let again = tools.call(&actor(), apply(&id)).await;
    assert_eq!(code_of(again), "mail_move_already_applied");
    assert_eq!(transport.posted().len(), 2, "the batch never runs twice");
}

#[tokio::test]
async fn excluding_a_message_keeps_the_digest_the_batch_was_asked_with() {
    let dir = TestDir::new("mail-move-exclude-digest");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            read("m2"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(
        &tools,
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Junk)],
    )
    .await;
    let digest = chat.approval(&id).expect("approval").action_digest;
    chat.decide_approval_excluding(
        &id,
        Decision::Approve,
        &["m2".to_owned()],
        "device-1",
        "device-1",
    )
    .expect("decide without m2");

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");

    assert_eq!(reply.result["status"], "applied", "not stale");
    assert_eq!(chat.approval(&id).expect("approval").action_digest, digest);
    assert_eq!(
        chat.approval(&id).expect("approval").status,
        ApprovalStatus::Consumed
    );
}
