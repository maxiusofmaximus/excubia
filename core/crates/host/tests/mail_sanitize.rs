//! The product path sanitizes text before it reaches Graph or the ledger: a
//! draft written by an agent and one edited by the owner. Over the routed fake
//! in `mail_send_support`; zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use hn_core_chat::SeatActor;
use hn_core_seat::{BodyFormat, DraftFields, DraftUpdate, MailCall};
use mail_send_support::{approved_and_sent, Host, ACCOUNT, BODY, DRAFT, TO};

const SUBJECT: &str = "Presupuesto de abril";
const SANITIZED_BODY: &str = "Hola Ana 👨\u{200D}👩\u{200D}👧";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn fields(subject: &str, body: &str) -> DraftFields {
    DraftFields {
        account: ACCOUNT.to_owned(),
        to: vec![TO.to_owned()],
        cc: Vec::new(),
        subject: subject.to_owned(),
        body: body.to_owned(),
        format: BodyFormat::Text,
    }
}

fn update(subject: &str, body: &str) -> MailCall {
    MailCall::DraftUpdate(DraftUpdate {
        draft_id: DRAFT.to_owned(),
        fields: fields(subject, body),
    })
}

#[tokio::test]
async fn a_draft_update_is_sanitized_before_graph_the_digest_and_the_review() {
    let host = Host::start("mail-sanitize-update").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service.clone());

    tools
        .call(
            &actor(),
            update(
                "Presu\u{200B}puesto de abril",
                "Hola\u{200B} Ana 👨\u{200D}👩\u{200D}👧",
            ),
        )
        .await
        .expect("the draft update");

    assert_eq!(host.world.subject(), SUBJECT);
    assert_eq!(host.world.body(), SANITIZED_BODY);

    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(review.subject, SUBJECT);
    assert_eq!(review.body, SANITIZED_BODY);

    approved_and_sent(&host, &service).await;
    assert_eq!(host.world.sends().len(), 1);
}

#[tokio::test]
async fn an_ascii_body_gets_one_final_hard_space_and_still_matches_the_review() {
    let host = Host::start("mail-sanitize-ascii").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service.clone());

    tools
        .call(&actor(), update(SUBJECT, "Hola Ana"))
        .await
        .expect("the draft update");

    assert_eq!(host.world.body(), "Hola Ana\u{00A0}");
    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(review.body, "Hola Ana");

    approved_and_sent(&host, &service).await;
    assert_eq!(host.world.sends().len(), 1);
}

#[tokio::test]
async fn a_new_draft_is_sanitized_too() {
    let host = Host::start("mail-sanitize-create").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service);

    let reply = tools
        .call(
            &actor(),
            MailCall::DraftCreate(fields("Presu\u{200B}puesto de abril", "Hola\u{200B} Ana")),
        )
        .await
        .expect("the draft create");

    assert_eq!(reply.result["draft"]["subject"], SUBJECT);
    assert_eq!(host.world.subject(), SUBJECT);
    assert_eq!(host.world.body(), "Hola Ana\u{00A0}");
}

#[tokio::test]
async fn a_body_without_visible_content_is_refused_before_graph() {
    let host = Host::start("mail-sanitize-invisible").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service);

    let refused = tools
        .call(&actor(), update(SUBJECT, "\u{200B}\u{FEFF}\u{2060}"))
        .await
        .expect_err("an invisible body is refused");

    assert_eq!(refused.code, "mail_invalid_input");
    assert_eq!(host.world.body(), format!("{BODY}\u{00A0}"));
}
