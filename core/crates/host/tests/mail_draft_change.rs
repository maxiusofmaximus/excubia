//! A change to a draft while the owner's question about it is open (MAIL2A2 debt,
//! MAIL2A3 §5a): the question settles as stale at the moment of the change, once,
//! and an answer that comes later cannot send the draft that was read. Over the
//! routed fake in `mail_send_support`. Zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use excubia_chat::{Decision, SeatActor};
use excubia_seat::{BodyFormat, DraftFields, DraftUpdate, MailCall};
use mail_send_support::{ask, Host, ACCOUNT, DRAFT, TO};

const CHANGED: &str = "Texto cambiado mientras el dueño decide.";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn update(body: &str) -> MailCall {
    MailCall::DraftUpdate(DraftUpdate {
        draft_id: DRAFT.to_owned(),
        fields: DraftFields {
            account: ACCOUNT.to_owned(),
            to: vec![TO.to_owned()],
            cc: Vec::new(),
            subject: "Presupuesto".to_owned(),
            body: body.to_owned(),
            format: BodyFormat::Text,
        },
    })
}

#[tokio::test]
async fn a_draft_update_settles_the_open_question_as_stale_at_once() {
    let host = Host::start("mail-draft-change").await;
    let service = host.service(Duration::from_millis(50), 10);
    let approval = ask(&service).await;
    let tools = host.tools(service.clone());

    // Graph now holds the new body, as it would after the update.
    host.world.set_body(CHANGED);
    tools
        .call(&actor(), update(CHANGED))
        .await
        .expect("the draft update");

    // The line is written by the change itself, before any answer arrives.
    assert_eq!(
        host.audit_text().matches("\"result\":\"stale\"").count(),
        1,
        "the change did not audit the question at once"
    );

    // The owner answers afterwards: the approval cannot send the draft that was read.
    host.decide(&approval, Decision::Approve);
    let refused = service
        .send(ACCOUNT, DRAFT, Some(&approval))
        .await
        .expect_err("a stale approval must not schedule a send");

    assert_eq!(refused.code(), "mail_approval_stale");
    assert!(host.world.sends().is_empty());
    assert_eq!(
        host.audit_text().matches("\"result\":\"stale\"").count(),
        1,
        "the question was audited more than once"
    );
}
