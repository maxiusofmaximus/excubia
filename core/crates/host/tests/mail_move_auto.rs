//! The host applies an approved mail batch when the operator decides it (F2b-4), over a
//! scripted Microsoft and the real chat store: the batch moves through the same path as
//! `mail.move_apply`, is burned once, and the conversation hears a count, never a subject.

mod mail_move_support;
mod mail_support;

use std::sync::Arc;

use excubia_chat::{ApprovalStatus, ChatStore, Decision};
use excubia_seat::MoveFolder;
use excubia_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;

/// What Alfred has said in `conversation`, oldest first.
fn alfred_said(chat: &ChatStore, conversation: &str) -> Vec<String> {
    chat.messages(conversation, None, None, 50)
        .expect("messages")
        .messages
        .into_iter()
        .filter(|message| message.author_kind == "agent")
        .map(|message| message.body_md)
        .collect()
}

#[tokio::test]
async fn an_approved_batch_moves_on_decision_and_the_conversation_hears_it() {
    let dir = TestDir::new("mail-move-auto-approve");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            read("m2"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            moved_from("INBOX-ID", "NEW-m2"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let conversation = open_conversation(&chat);
    let id = ask_in(
        &tools,
        &conversation,
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Junk)],
    )
    .await;

    chat.decide_approval_excluding(&id, Decision::Approve, &[], "device-1", "device-1")
        .expect("decide");
    let approval = chat.approval(&id).expect("approval");
    tools.on_decided(&approval).await;

    let posted = transport.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert_eq!(
        action_log(&dir)
            .list_recent(ACCOUNT, 10)
            .expect("log")
            .len(),
        2
    );
    assert_eq!(
        chat.approval(&id).expect("approval").status,
        ApprovalStatus::Consumed
    );
    assert_eq!(
        alfred_said(&chat, &conversation),
        ["Moví 2 correos a no deseado."]
    );
}

#[tokio::test]
async fn the_owner_left_out_is_counted_and_not_moved() {
    let dir = TestDir::new("mail-move-auto-exclude");
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
    let conversation = open_conversation(&chat);
    let id = ask_in(
        &tools,
        &conversation,
        &[
            ("m1", MoveFolder::Junk),
            ("m2", MoveFolder::Junk),
            ("m3", MoveFolder::Deleted),
        ],
    )
    .await;
    chat.decide_approval_excluding(
        &id,
        Decision::Approve,
        &["m2".to_owned()],
        "device-1",
        "device-1",
    )
    .expect("decide without m2");

    tools
        .on_decided(&chat.approval(&id).expect("approval"))
        .await;

    let posted = transport.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert!(posted.iter().all(|url| !url.contains("/m2/")), "{posted:?}");
    assert_eq!(
        alfred_said(&chat, &conversation),
        ["Moví 2 correos a no deseado, eliminados. Dejaste fuera: 1."]
    );
}

#[tokio::test]
async fn a_denied_batch_moves_nothing_and_says_nothing() {
    let dir = TestDir::new("mail-move-auto-deny");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted([vec![token_answer()], read("m1")].concat());
    let tools = host(&dir, &transport, &chat);
    let conversation = open_conversation(&chat);
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval_excluding(&id, Decision::Deny, &[], "device-1", "device-1")
        .expect("deny");

    tools
        .on_decided(&chat.approval(&id).expect("approval"))
        .await;

    assert!(transport.posted().is_empty());
    assert!(alfred_said(&chat, &conversation).is_empty());
}

#[tokio::test]
async fn a_repeated_decision_applies_nothing_twice() {
    let dir = TestDir::new("mail-move-auto-repeat");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let conversation = open_conversation(&chat);
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;

    for _ in 0..2 {
        chat.decide_approval_excluding(&id, Decision::Approve, &[], "client-1", "device-1")
            .expect("the same decision answers again");
    }
    for _ in 0..2 {
        tools
            .on_decided(&chat.approval(&id).expect("approval"))
            .await;
    }

    assert_eq!(transport.posted().len(), 1, "{:?}", transport.posted());
    assert_eq!(alfred_said(&chat, &conversation).len(), 1);
}

#[tokio::test]
async fn a_failed_move_does_not_stop_the_rest_and_the_report_counts_it() {
    let dir = TestDir::new("mail-move-auto-fail");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            read("m2"),
            read("m3"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            vec![missing()],
            moved_from("INBOX-ID", "NEW-m3"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let conversation = open_conversation(&chat);
    let id = ask_in(
        &tools,
        &conversation,
        &[
            ("m1", MoveFolder::Junk),
            ("m2", MoveFolder::Junk),
            ("m3", MoveFolder::Deleted),
        ],
    )
    .await;
    chat.decide_approval_excluding(&id, Decision::Approve, &[], "device-1", "device-1")
        .expect("decide");

    tools
        .on_decided(&chat.approval(&id).expect("approval"))
        .await;

    assert_eq!(transport.posted().len(), 2, "{:?}", transport.posted());
    assert_eq!(
        alfred_said(&chat, &conversation),
        ["Moví 2 correos a no deseado, eliminados. Fallos: 1."]
    );
}

#[tokio::test]
async fn a_batch_that_cannot_run_says_its_stable_code_and_no_content() {
    let dir = TestDir::new("mail-move-auto-total");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    // The token of the apply is refused: nothing is burned and nothing moves.
    let transport = scripted([vec![token_answer()], read("m1"), vec![missing()]].concat());
    let tools = host(&dir, &transport, &chat);
    let conversation = open_conversation(&chat);
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval_excluding(&id, Decision::Approve, &[], "device-1", "device-1")
        .expect("decide");

    tools
        .on_decided(&chat.approval(&id).expect("approval"))
        .await;

    let said = alfred_said(&chat, &conversation);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with("No pude mover el lote de correos: mail_"),
        "{said:?}"
    );
    assert!(
        !said[0].contains("promo") && !said[0].contains("Oferta"),
        "{said:?}"
    );
    assert_eq!(
        chat.approval(&id).expect("approval").status,
        ApprovalStatus::Approved
    );
}

#[tokio::test]
async fn another_tools_approval_is_not_applied_here() {
    let dir = TestDir::new("mail-move-auto-other-tool");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let id = raw_approval(&chat, "mail.send", ACCOUNT, "digest-1");
    chat.decide_approval_excluding(&id, Decision::Approve, &[], "device-1", "device-1")
        .expect("decide");

    tools
        .on_decided(&chat.approval(&id).expect("approval"))
        .await;

    assert!(transport.posted().is_empty());
    assert_eq!(
        chat.approval(&id).expect("approval").status,
        ApprovalStatus::Approved
    );
}

#[tokio::test]
async fn the_decision_does_not_wait_for_microsoft() {
    let dir = TestDir::new("mail-move-auto-background");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
        ]
        .concat(),
    );
    let tools = Arc::new(host(&dir, &transport, &chat));
    let conversation = open_conversation(&chat);
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval_excluding(&id, Decision::Approve, &[], "device-1", "device-1")
        .expect("decide");

    tools.decided_in_background(chat.approval(&id).expect("approval"));

    assert!(
        transport.posted().is_empty(),
        "the answer goes before the batch runs"
    );
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(transport.posted().len(), 1);
    assert_eq!(
        alfred_said(&chat, &conversation),
        ["Moví 1 correo a no deseado."]
    );
}
