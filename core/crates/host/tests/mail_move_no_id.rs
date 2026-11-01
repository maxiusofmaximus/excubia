//! The organize tools without an approval id (F2b-3): `mail.move_apply` applies the one
//! approved batch of the conversation, names the ambiguity when there are several, and
//! moves nothing while there is none. Same scripted Microsoft and real chat store as
//! `mail_move.rs`.

mod mail_move_support;
mod mail_support;

use hn_core_chat::{ApprovalKind, ChatStore, Decision, NewApproval};
use hn_core_seat::MoveFolder;
use hn_core_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// A question of `tool` for `account`, asked in `conversation`, with a digest of its own.
fn new_in(conversation: &str, tool: &str, account: &str, digest: &str, ttl: u32) -> NewApproval {
    NewApproval {
        conversation_id: Some(conversation.to_owned()),
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: tool.to_owned(),
        summary: "prueba".to_owned(),
        cwd: None,
        paths: Vec::new(),
        risk: None,
        action_digest: digest.to_owned(),
        payload_redacted: Some(json!({"account": account, "items": []})),
        ttl_seconds: ttl,
    }
}

#[tokio::test]
async fn apply_without_an_id_applies_the_one_approved_batch_of_the_conversation() {
    let dir = TestDir::new("mail-move-no-id");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
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
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    let reply = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("apply");

    assert_eq!(reply.result["status"], "applied");
    assert_eq!(reply.result["results"][0]["new_message_id"], "NEW-m1");
    assert_eq!(transport.posted().len(), 1);
    let log = action_log(&dir).list_recent(ACCOUNT, 10).expect("log");
    assert!(log
        .iter()
        .all(|record| record.approval_id.as_deref() == Some(id.as_str())));
}

#[tokio::test]
async fn without_an_approved_batch_nothing_moves_and_pending_ones_are_counted() {
    let dir = TestDir::new("mail-move-nothing-approved");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
    let transport = scripted([vec![token_answer()], read("m1")].concat());
    let tools = host(&dir, &transport, &chat);
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;

    let waiting = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("answer");
    assert_eq!(waiting.result["status"], "nothing_approved");
    assert_eq!(waiting.result["pending"], 1);

    chat.decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("decide");
    let denied = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("answer");
    assert_eq!(denied.result["status"], "nothing_approved");
    assert_eq!(denied.result["pending"], 0);
    assert!(transport.posted().is_empty(), "nothing moved");
}

#[tokio::test]
async fn approved_batches_of_another_account_tool_or_conversation_do_not_count() {
    let dir = TestDir::new("mail-move-not-ours");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
    let elsewhere = open_conversation(&chat);
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let approvals = [
        chat.create_mail_organize_approval(new_in(
            &conversation,
            "mail.move",
            "otra",
            "d-cuenta",
            1800,
        )),
        chat.create_mail_send_approval(new_in(
            &conversation,
            "mail.send",
            ACCOUNT,
            "d-envio",
            1800,
        )),
        chat.create_mail_organize_approval(new_in(
            &elsewhere,
            "mail.move",
            ACCOUNT,
            "d-charla",
            1800,
        )),
    ];
    for approval in approvals {
        let id = approval.expect("approval").id;
        chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
            .expect("decide");
    }

    let reply = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("answer");

    assert_eq!(reply.result["status"], "nothing_approved");
    assert_eq!(reply.result["pending"], 0);
    assert!(transport.posted().is_empty());
}

#[tokio::test]
async fn two_approved_batches_are_ambiguous_and_nothing_moves_until_one_is_named() {
    let dir = TestDir::new("mail-move-ambiguous");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            read("m2"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m2"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let first = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    let second = ask_in(&tools, &conversation, &[("m2", MoveFolder::Junk)]).await;
    for id in [&first, &second] {
        chat.decide_approval(id, Decision::Approve, "device-1", "device-1")
            .expect("decide");
    }

    let refused = tools.call(&actor_in(&conversation), apply_latest()).await;

    match refused {
        Ok(_) => panic!("two batches must not be guessed"),
        Err(error) => {
            assert_eq!(error.code, "ambiguous_approval");
            assert_eq!(error.field, Some("approval_id"));
            assert!(error.message.contains(&first) && error.message.contains(&second));
        }
    }
    assert!(transport.posted().is_empty());

    tools
        .call(&actor_in(&conversation), apply(&first))
        .await
        .expect("the named batch applies");
    let last = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("the one left applies");
    assert_eq!(last.result["results"][0]["new_message_id"], "NEW-m2");
    assert_eq!(transport.posted().len(), 2);
}

#[tokio::test]
async fn a_consumed_batch_is_not_approved_again() {
    let dir = TestDir::new("mail-move-consumed");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
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
    let id = ask_in(&tools, &conversation, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    tools
        .call(&actor_in(&conversation), apply(&id))
        .await
        .expect("apply");

    let again = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("answer");

    assert_eq!(again.result["status"], "nothing_approved");
    assert_eq!(transport.posted().len(), 1, "the batch never runs twice");
}

#[tokio::test]
async fn an_approved_batch_past_its_deadline_is_not_applied() {
    let dir = TestDir::new("mail-move-deadline");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let id = chat
        .create_mail_organize_approval(new_in(&conversation, "mail.move", ACCOUNT, "d-tarde", 1))
        .expect("approval")
        .id;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    std::thread::sleep(Duration::from_millis(1100));

    let late = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("answer");

    assert_eq!(late.result["status"], "nothing_approved");
    assert!(transport.posted().is_empty());
}

#[tokio::test]
async fn the_owner_exclusions_still_hold_without_an_id() {
    let dir = TestDir::new("mail-move-excluded-no-id");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = open_conversation(&chat);
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
    let id = ask_in(
        &tools,
        &conversation,
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Junk)],
    )
    .await;
    chat.decide_approval_excluding(
        &id,
        Decision::Approve,
        &["m2".to_owned()],
        "device-1",
        "device-1",
    )
    .expect("decide");

    let reply = tools
        .call(&actor_in(&conversation), apply_latest())
        .await
        .expect("apply");

    assert_eq!(reply.result["results"][1]["skipped"], "owner");
    assert_eq!(reply.result["results"][0]["new_message_id"], "NEW-m1");
    assert_eq!(transport.posted().len(), 1, "the excluded one never moves");
}
