//! The file question of MAIL1C §1 over the chat approval flow: a device answers
//! it, a seat cannot, and a question nobody answers in time expires.

use std::path::PathBuf;
use std::time::Duration;

use excubia_chat::{
    ApprovalFilter, ApprovalStatus, ChatStore, Decision, NewConversation, SeatActor,
};
use excubia_host::mail_approval::{ChatApprovalGate, ACTION};
use excubia_mail::{Approval, ApprovalGate, CheckedFile, PathApproval};

const OUTSIDE: &str = "C:/fuera/informe.xlsx";

fn request() -> PathApproval {
    PathApproval {
        digest: "ab".repeat(32),
        file: CheckedFile {
            path: PathBuf::from(OUTSIDE),
            size: 2048,
            sha256: "cd".repeat(32),
        },
    }
}

fn pending(chat: &ChatStore) -> Vec<excubia_chat::Approval> {
    chat.approvals(&ApprovalFilter {
        status: Some(ApprovalStatus::Pending),
        conversation_id: None,
        limit: 10,
    })
    .expect("list approvals")
}

/// Waits until the gate has filed its question, then returns its id.
async fn next_pending(chat: &ChatStore) -> String {
    for _ in 0..500 {
        if let Some(first) = pending(chat).first() {
            return first.id.clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the question never reached the inbox");
}

#[tokio::test]
async fn a_device_that_approves_lets_the_file_go() {
    let chat = ChatStore::in_memory().expect("store");
    let gate = ChatApprovalGate::new(&chat, "C:/proyecto");
    let asked = request();

    let deciding = async {
        let id = next_pending(&chat).await;
        chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
            .expect("the device decides");
    };
    let (answer, ()) = tokio::join!(gate.decide(&asked), deciding);

    assert_eq!(answer.expect("an answer"), Approval::Approved);
}

#[tokio::test]
async fn a_device_that_denies_keeps_the_file_out() {
    let chat = ChatStore::in_memory().expect("store");
    let gate = ChatApprovalGate::new(&chat, "C:/proyecto");
    let asked = request();

    let deciding = async {
        let id = next_pending(&chat).await;
        chat.decide_approval(&id, Decision::Deny, "device-1", "device-1")
            .expect("the device decides");
    };
    let (answer, ()) = tokio::join!(gate.decide(&asked), deciding);

    assert_eq!(answer.expect("an answer"), Approval::Denied);
}

#[tokio::test]
async fn a_question_nobody_answers_expires() {
    let chat = ChatStore::in_memory().expect("store");
    let gate = ChatApprovalGate::new(&chat, "C:/proyecto").with_ttl(1);

    let answer = gate.decide(&request()).await.expect("an answer");

    assert_eq!(answer, Approval::Expired);
    let expired = chat
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Expired),
            conversation_id: None,
            limit: 10,
        })
        .expect("list approvals");
    assert_eq!(expired.len(), 1, "the question expired in the inbox");
    assert_eq!(expired[0].tool, ACTION);
}

#[tokio::test]
async fn a_seat_cannot_answer_the_file_question() {
    let chat = ChatStore::in_memory().expect("store");
    let conversation = chat
        .create_conversation(NewConversation {
            title: Some("Correo".to_owned()),
            project_id: None,
        })
        .expect("conversation");
    let gate = ChatApprovalGate::new(&chat, "C:/proyecto").in_conversation(&conversation.id);
    let asked = request();
    let seat = SeatActor {
        agent_id: chat.bootstrap().expect("bootstrap").alfred.agent_id,
        role_id: "rol-correo".to_owned(),
        project_id: None,
        conversation_id: conversation.id.clone(),
        run_id: "run-ajeno".to_owned(),
    };

    let deciding = async {
        let id = next_pending(&chat).await;
        let refused = chat.decide_delegated(&seat, &id, Decision::Approve, None);
        assert!(refused.is_err(), "a seat must not approve its own file");
        chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
            .expect("the device decides");
    };
    let (answer, ()) = tokio::join!(gate.decide(&asked), deciding);

    assert_eq!(answer.expect("an answer"), Approval::Approved);
}
