//! A draft whose body promises a file the draft does not carry answers with a
//! warning (TASK-FIX1 §2); a body that promises none gets no field. Over the routed
//! fake in `mail_send_support`. Zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use excubia_chat::SeatActor;
use excubia_seat::{BodyFormat, DraftFields, MailCall};
use mail_send_support::{Host, ACCOUNT, TO};

const WARNING: &str = "El cuerpo menciona un adjunto pero el borrador no lleva ninguno. Usa mail.attach_file o quita la frase.";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn create(body: &str) -> MailCall {
    MailCall::DraftCreate(DraftFields {
        account: ACCOUNT.to_owned(),
        to: vec![TO.to_owned()],
        cc: Vec::new(),
        subject: "Presupuesto".to_owned(),
        body: body.to_owned(),
        format: BodyFormat::Text,
    })
}

#[tokio::test]
async fn a_body_that_promises_a_file_the_draft_lacks_gets_a_warning() {
    let host = Host::start("mail-draft-warning").await;
    let tools = host.tools(host.service(Duration::from_millis(50), 10));

    let reply = tools
        .call(&actor(), create("Te envío el archivo mañana."))
        .await
        .expect("the draft create");

    assert_eq!(reply.result["ok"], true);
    assert_eq!(reply.result["warnings"][0], WARNING);
}

#[tokio::test]
async fn a_body_that_promises_no_file_gets_no_warning() {
    let host = Host::start("mail-draft-no-warning").await;
    let tools = host.tools(host.service(Duration::from_millis(50), 10));

    let reply = tools
        .call(&actor(), create("Nos vemos mañana."))
        .await
        .expect("the draft create");

    assert!(reply.result.get("warnings").is_none(), "{}", reply.result);
}
