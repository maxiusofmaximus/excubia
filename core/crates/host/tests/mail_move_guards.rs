//! The checks an apply makes before it moves anything (a digest that no longer matches,
//! another tool's or account's approval) and the undo of a logged move: each refusal is
//! a stable code, and an approval or an action is spent once.

mod mail_move_support;
mod mail_support;

use std::sync::Arc;

use excubia_core_chat::{ChatStore, Decision};
use excubia_core_seat::{MailCall, MoveFolder, MoveUndo, Organize};
use excubia_core_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;

#[tokio::test]
async fn an_approval_whose_digest_does_not_match_its_batch_is_stale_and_spent() {
    let dir = TestDir::new("mail-move-digest");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let id = raw_approval(&chat, "mail.move", ACCOUNT, "sha256:otro-lote");
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    let stale = tools.call(&actor(), apply(&id)).await;
    assert_eq!(code_of(stale), "mail_move_stale");
    let again = tools.call(&actor(), apply(&id)).await;
    assert_eq!(code_of(again), "mail_move_already_applied");
    assert!(transport.posted().is_empty());
}

#[tokio::test]
async fn an_approval_of_another_tool_or_account_is_not_a_batch_of_this_account() {
    let dir = TestDir::new("mail-move-other");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let other_tool = raw_approval(&chat, "mail.send", ACCOUNT, "sha256:a");
    let other_account = raw_approval(&chat, "mail.move", "otro", "sha256:b");
    for id in [&other_tool, &other_account] {
        chat.decide_approval(id, Decision::Approve, "device-1", "device-1")
            .expect("decide");
        let refused = tools.call(&actor(), apply(id)).await;
        assert_eq!(code_of(refused), "mail_invalid_input");
    }
    assert!(transport.posted().is_empty());
}

#[tokio::test]
async fn undo_puts_a_moved_message_back_once_and_only_when_it_was_logged() {
    let dir = TestDir::new("mail-move-undo");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            vec![token_answer()],
            moved_from("JUNK-ID", "BACK-m1"),
            vec![token_answer()],
            vec![token_answer()],
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(&tools, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    let applied = tools.call(&actor(), apply(&id)).await.expect("apply");
    let action = applied.result["results"][0]["action_id"]
        .as_str()
        .expect("action")
        .to_owned();
    let undo = |action_id: &str| {
        MailCall::Organize(Organize::Undo(MoveUndo {
            account: ACCOUNT.to_owned(),
            action_id: action_id.to_owned(),
        }))
    };

    let restored = tools.call(&actor(), undo(&action)).await.expect("undo");
    assert_eq!(restored.result["restored_message_id"], "BACK-m1");
    assert!(transport
        .posted()
        .last()
        .expect("post")
        .ends_with("/me/messages/NEW-m1/move"));

    let twice = tools.call(&actor(), undo(&action)).await;
    assert_eq!(code_of(twice), "mail_action_undone");
    let unknown = tools.call(&actor(), undo("no-existe")).await;
    assert_eq!(code_of(unknown), "mail_action_not_found");
}
