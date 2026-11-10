//! The organize tools of the mail surface (F2b2) over a scripted Microsoft and the real
//! chat store: a batch is proposed and nothing moves, and the operator's decision decides
//! whether it moves. A decided batch moves once, in order, and each move is logged, and
//! a failed move does not stop the rest.

mod mail_move_support;
mod mail_support;

use excubia_core_chat::{ApprovalStatus, ChatStore, Decision};
use excubia_core_seat::MoveFolder;
use excubia_core_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;
use std::sync::Arc;

#[tokio::test]
async fn proposing_reads_each_message_and_asks_once_without_moving() {
    let dir = TestDir::new("mail-move-propose");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted([vec![token_answer()], read("m1"), read("m2")].concat());
    let tools = host(&dir, &transport, &chat);

    let reply = tools
        .call(
            &actor(),
            propose(&[("m1", MoveFolder::Junk), ("m2", MoveFolder::Inbox)]),
        )
        .await
        .expect("propose");

    let result = &reply.result;
    assert_eq!(result["status"], "pending");
    assert_eq!(result["count"], 2);
    assert_eq!(result["expires_in"], 1800);
    let approval = chat
        .approval(result["approval_id"].as_str().expect("id"))
        .expect("approval");
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.tool, "mail.move");
    let payload = approval.payload_redacted.expect("payload");
    assert_eq!(payload["items"][0]["dest"], "junkemail");
    assert_eq!(payload["items"][0]["reason"], "Publicidad");
    assert_eq!(payload["items"][0]["from_address"], "promo@tienda.example");
    assert!(payload["items"][0]["level"].is_string());
    assert!(!payload.to_string().contains("CUERPO-SECRETO"), "no body");
    assert!(
        transport.posted().is_empty(),
        "nothing moves before the decision"
    );
    let audit = written(&dir.join("data"));
    assert!(audit.contains("mail.move_propose"), "each read is audited");
    assert!(!audit.contains("confidencial"), "no subject in the audit");
}

#[tokio::test]
async fn a_batch_whose_message_cannot_be_read_is_refused_and_asks_nothing() {
    let dir = TestDir::new("mail-move-unreadable");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(vec![token_answer(), missing()]);
    let tools = host(&dir, &transport, &chat);

    let refused = tools
        .call(&actor(), propose(&[("m1", MoveFolder::Junk)]))
        .await;

    assert_eq!(code_of(refused), "mail_not_found");
    assert_eq!(chat.pending_approval_count().expect("count"), 0);
}

#[tokio::test]
async fn applying_before_the_decision_moves_nothing() {
    let dir = TestDir::new("mail-move-pending");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted([vec![token_answer()], read("m1")].concat());
    let tools = host(&dir, &transport, &chat);
    let id = ask(&tools, &[("m1", MoveFolder::Junk)]).await;

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");

    assert_eq!(reply.result["status"], "pending");
    assert!(transport.posted().is_empty());
}

#[tokio::test]
async fn an_approved_batch_moves_each_message_in_order_and_logs_each_move() {
    let dir = TestDir::new("mail-move-apply");
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
    let id = ask(
        &tools,
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Deleted)],
    )
    .await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");

    let results = &reply.result["results"];
    assert_eq!(reply.result["status"], "applied");
    assert_eq!(results[0]["new_message_id"], "NEW-m1");
    assert_eq!(results[0]["dest"], "junkemail");
    assert_eq!(results[1]["new_message_id"], "NEW-m2");
    assert_eq!(results[1]["dest"], "deleteditems");
    let posted = transport.posted();
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert!(posted[0].ends_with("/me/messages/m1/move"), "{posted:?}");
    assert!(posted[1].ends_with("/me/messages/m2/move"), "{posted:?}");
    let log = action_log(&dir).list_recent(ACCOUNT, 10).expect("log");
    assert_eq!(log.len(), 2);
    assert!(log
        .iter()
        .all(|record| record.approval_id.as_deref() == Some(id.as_str())
            && record.decider_kind.as_deref() == Some("owner")));
    let first = results[0]["action_id"].as_str().expect("action");
    assert!(log.iter().any(|record| record.id == first));

    let again = tools.call(&actor(), apply(&id)).await;
    assert_eq!(code_of(again), "mail_move_already_applied");
    assert_eq!(transport.posted().len(), 2, "the batch never runs twice");
}

#[tokio::test]
async fn a_message_that_cannot_move_does_not_stop_the_rest_of_the_batch() {
    let dir = TestDir::new("mail-move-partial");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            read("m2"),
            vec![token_answer(), missing()],
            moved_from("INBOX-ID", "NEW-m2"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(
        &tools,
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Junk)],
    )
    .await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");

    let results = &reply.result["results"];
    assert_eq!(results[0]["error"], "mail_not_found");
    assert_eq!(results[1]["new_message_id"], "NEW-m2");
    assert_eq!(
        action_log(&dir)
            .list_recent(ACCOUNT, 10)
            .expect("log")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_denied_batch_moves_nothing_and_cannot_be_asked_again_as_it_was() {
    let dir = TestDir::new("mail-move-denied");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            read("m1"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(&tools, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("decide");

    let reply = tools.call(&actor(), apply(&id)).await.expect("apply");
    assert_eq!(reply.result["status"], "denied");
    assert!(transport.posted().is_empty());

    let asked_again = tools
        .call(&actor(), propose(&[("m1", MoveFolder::Junk)]))
        .await;
    assert_eq!(code_of(asked_again), "mail_move_refused");
}

#[tokio::test]
async fn the_same_open_batch_is_one_question_and_an_applied_one_is_stale() {
    let dir = TestDir::new("mail-move-twice");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            vec![token_answer()],
            read("m1"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(&tools, &[("m1", MoveFolder::Junk)]).await;

    let open_again = ask(&tools, &[("m1", MoveFolder::Junk)]).await;
    assert_eq!(open_again, id, "the open question answers with itself");

    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    tools.call(&actor(), apply(&id)).await.expect("apply");
    let asked_after = tools
        .call(&actor(), propose(&[("m1", MoveFolder::Junk)]))
        .await;
    assert_eq!(code_of(asked_after), "mail_move_stale");
}
