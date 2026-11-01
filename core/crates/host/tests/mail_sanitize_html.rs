//! The HTML of an owner's draft is cleaned before it reaches Graph; an agent's
//! template is the host's own markup and is never cleaned. Over the routed fake in
//! `mail_send_support`: zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use hn_core_chat::SeatActor;
use hn_core_seat::{BodyFormat, DraftFields, MailCall};
use mail_send_support::{Host, ACCOUNT, TO};

const SUBJECT: &str = "Presupuesto de abril";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn create(format: BodyFormat, body: &str) -> MailCall {
    MailCall::DraftCreate(DraftFields {
        account: ACCOUNT.to_owned(),
        to: vec![TO.to_owned()],
        cc: Vec::new(),
        subject: SUBJECT.to_owned(),
        body: body.to_owned(),
        format,
    })
}

#[tokio::test]
async fn an_html_draft_reaches_graph_without_its_script() {
    let host = Host::start("mail-sanitize-html-create").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service);

    tools
        .call(
            &actor(),
            create(
                BodyFormat::Html,
                "<p>Hola <b>Ana</b></p><script>alert(1)</script>",
            ),
        )
        .await
        .expect("the draft create");

    assert_eq!(host.world.body(), "<p>Hola <b>Ana</b></p>");
}

#[tokio::test]
async fn an_agent_template_is_drawn_and_never_cleaned() {
    let host = Host::start("mail-sanitize-agent-kept").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service);

    tools
        .call(&actor(), create(BodyFormat::Agent, "Hola Ana"))
        .await
        .expect("the agent draft create");

    let body = host.world.body();
    assert!(body.contains("<!--excubia:agent:v1-->"), "{body}");
    assert!(
        body.contains("style="),
        "the template's own styles were cleaned: {body}"
    );
}
