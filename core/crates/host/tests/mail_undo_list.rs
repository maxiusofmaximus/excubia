//! `mail.undo_list` (F2B2): the recent moves that can still be undone, with the ids that
//! `mail.undo` takes. It reads the local log only: no subject, sender or body comes back,
//! another account is refused, and an undone move leaves the list.

mod mail_move_support;
mod mail_support;

use std::sync::Arc;

use excubia_core_chat::{ChatStore, Decision};
use excubia_core_seat::{MailCall, MoveFolder, MoveUndo, Organize, UndoList};
use excubia_core_test_dir::TestDir;
use mail_move_support::*;
use mail_support::token_answer;
use serde_json::json;

fn listed(account: &str, n: u32) -> MailCall {
    MailCall::Organize(Organize::UndoList(UndoList {
        account: account.to_owned(),
        n,
    }))
}

fn undo(action_id: &str) -> MailCall {
    MailCall::Organize(Organize::Undo(MoveUndo {
        account: ACCOUNT.to_owned(),
        action_id: action_id.to_owned(),
    }))
}

#[tokio::test]
async fn the_listed_id_is_the_one_undo_takes_and_an_undone_move_leaves_the_list() {
    let dir = TestDir::new("mail-undo-list");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(
        [
            vec![token_answer()],
            read("m1"),
            vec![token_answer()],
            moved_from("INBOX-ID", "NEW-m1"),
            vec![token_answer()],
            moved_from("JUNK-ID", "BACK-m1"),
        ]
        .concat(),
    );
    let tools = host(&dir, &transport, &chat);
    let id = ask(&tools, &[("m1", MoveFolder::Junk)]).await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    tools.call(&actor(), apply(&id)).await.expect("apply");

    let before = tools
        .call(&actor(), listed(ACCOUNT, 10))
        .await
        .expect("list");
    let moves = &before.result["actions"];
    assert_eq!(moves.as_array().map(Vec::len), Some(1));
    assert_eq!(moves[0]["dest"], "junkemail");
    assert_eq!(moves[0]["undone"], false);
    assert!(moves[0]["at"].is_string());
    let text = before.result.to_string();
    for private in ["Oferta", "promo@tienda", "CUERPO-SECRETO", "ana@x.com"] {
        assert!(!text.contains(private), "{private}");
    }

    let action = moves[0]["action_id"].as_str().expect("action").to_owned();
    let restored = tools.call(&actor(), undo(&action)).await.expect("undo");
    assert_eq!(restored.result["restored_message_id"], "BACK-m1");
    let after = tools
        .call(&actor(), listed(ACCOUNT, 10))
        .await
        .expect("list");
    assert_eq!(after.result["actions"], json!([]));
}

#[tokio::test]
async fn the_newest_moves_come_first_and_n_caps_the_list() {
    let dir = TestDir::new("mail-undo-list-n");
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
        &[("m1", MoveFolder::Junk), ("m2", MoveFolder::Junk)],
    )
    .await;
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    let applied = tools.call(&actor(), apply(&id)).await.expect("apply");
    let first = applied.result["results"][0]["action_id"]
        .as_str()
        .expect("first")
        .to_owned();
    let second = applied.result["results"][1]["action_id"]
        .as_str()
        .expect("second")
        .to_owned();

    let newest = tools
        .call(&actor(), listed(ACCOUNT, 1))
        .await
        .expect("list");
    assert_eq!(newest.result["actions"].as_array().map(Vec::len), Some(1));
    assert_eq!(newest.result["actions"][0]["action_id"], second.as_str());
    let all = tools
        .call(&actor(), listed(ACCOUNT, 10))
        .await
        .expect("list");
    assert_eq!(all.result["actions"][0]["action_id"], second.as_str());
    assert_eq!(all.result["actions"][1]["action_id"], first.as_str());
}

#[tokio::test]
async fn another_account_is_refused_and_nothing_is_sent() {
    let dir = TestDir::new("mail-undo-list-other");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);
    let refused = tools.call(&actor(), listed("otro", 10)).await;
    assert_eq!(code_of(refused), "mail_invalid_input");
    assert!(transport.posted().is_empty());
}
