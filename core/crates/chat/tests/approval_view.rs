//! What each reader of an approval sees (MAIL2A3 §2): the mail actions carry their
//! `details` only for the operator, and the payload of any other tool is left as
//! it was for everyone.

use hn_core_chat::{ApprovalKind, ChatStore, NewApproval, Reader};
use serde_json::{json, Value};

fn question(tool: &str, payload: Value) -> NewApproval {
    NewApproval {
        conversation_id: None,
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: tool.to_owned(),
        summary: "Enviar «Presupuesto» a 1 destinatario(s)".to_owned(),
        cwd: None,
        paths: Vec::new(),
        risk: None,
        action_digest: format!("digest-{tool}"),
        payload_redacted: Some(payload),
        ttl_seconds: 120,
    }
}

#[test]
fn only_the_operator_sees_the_details_of_a_mail_send() {
    let chat = ChatStore::in_memory().expect("chat");
    let payload = json!({"account": "trabajo", "to": ["ana@example.org"], "cc": []});
    let approval = chat
        .create_mail_send_approval(question("mail.send", payload.clone()))
        .expect("the approval");

    let operator = approval.view(Reader::Operator);
    assert_eq!(operator["details"], payload);
    assert!(operator.get("payload_redacted").is_none(), "{operator}");

    let view = approval.view(Reader::Other);
    assert!(view.get("details").is_none(), "{view}");
    assert!(view.get("payload_redacted").is_none(), "{view}");
    assert!(!view.to_string().contains("ana@example.org"), "{view}");
}

#[test]
fn the_details_of_a_file_attached_from_outside_reach_the_operator_only() {
    let chat = ChatStore::in_memory().expect("chat");
    let payload = json!({"path": "C:/fuera/presupuesto.pdf", "size": 12, "sha256": "ab"});
    let approval = chat
        .create_outside_approval(question("mail.attach_path", payload.clone()))
        .expect("the approval");

    assert_eq!(approval.view(Reader::Operator)["details"], payload);
    assert!(approval.view(Reader::Other).get("details").is_none());
}

#[test]
fn another_tools_payload_is_left_as_it_was_for_every_reader() {
    let chat = ChatStore::in_memory().expect("chat");
    let payload = json!({"command": "cargo test"});
    let approval = chat
        .create_approval(question("shell.run", payload.clone()))
        .expect("the approval");

    for reader in [Reader::Operator, Reader::Other] {
        let view = approval.view(reader);
        assert_eq!(view["payload_redacted"], payload, "{view}");
        assert!(view.get("details").is_none(), "{view}");
    }
}
