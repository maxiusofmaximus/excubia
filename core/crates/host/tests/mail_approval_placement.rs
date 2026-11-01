//! The file question is placed for the operator alone (MAIL1D §0, NV-2): the
//! host filed it as outside the workspace, so no seat decides it.

use std::path::PathBuf;
use std::time::Duration;

use hn_core_chat::{ApprovalFilter, ApprovalStatus, ChatStore, Decision};
use hn_core_host::mail_approval::ChatApprovalGate;
use hn_core_mail::{Approval, ApprovalGate, CheckedFile, PathApproval};

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

/// Waits until the gate has filed its question, then returns it.
async fn next_pending(chat: &ChatStore) -> hn_core_chat::Approval {
    for _ in 0..500 {
        let pending = chat
            .approvals(&ApprovalFilter {
                status: Some(ApprovalStatus::Pending),
                conversation_id: None,
                limit: 10,
            })
            .expect("list approvals");
        if let Some(first) = pending.into_iter().next() {
            return first;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the question never reached the inbox");
}

#[tokio::test]
async fn the_question_is_placed_for_the_operator_alone() {
    let chat = ChatStore::in_memory().expect("store");
    let gate = ChatApprovalGate::new(&chat, "C:/proyecto");
    let asked = request();

    let deciding = async {
        let filed = next_pending(&chat).await;
        assert_eq!(filed.level.as_deref(), Some("user"));
        assert_eq!(filed.reason.as_deref(), Some("never_delegate"));
        chat.decide_approval(&filed.id, Decision::Deny, "device-1", "device-1")
            .expect("the device decides");
    };
    let (answer, ()) = tokio::join!(gate.decide(&asked), deciding);

    assert_eq!(answer.expect("an answer"), Approval::Denied);
}
