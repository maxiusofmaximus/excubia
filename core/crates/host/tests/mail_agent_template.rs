//! An agent's mail in the fixed layout, through the tools: the draft Graph holds
//! is the HTML the template renders, the owner reads what the recipient gets, and
//! the approval goes through with the body it read. Over the routed fake in
//! `mail_send_support`. Zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use excubia_api::DraftEdit;
use excubia_chat::SeatActor;
use excubia_host::mail_send::MailSends;
use excubia_seat::{BodyFormat, DraftFields, MailCall};
use mail_send_support::{approved_and_sent, Host, ACCOUNT, DRAFT, TO};

const SUBJECT: &str = "Resumen de la reunión";
const NEW_SUBJECT: &str = "Presupuesto de abril";
const BODY: &str =
    "Hola Ana,\nte resumo la reunión.\n\nSegundo párrafo con tildes 👨\u{200D}👩\u{200D}👧.";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn agent_draft_with(body: &str) -> MailCall {
    MailCall::DraftCreate(DraftFields {
        account: ACCOUNT.to_owned(),
        to: vec![TO.to_owned()],
        cc: Vec::new(),
        subject: SUBJECT.to_owned(),
        body: body.to_owned(),
        format: BodyFormat::Agent,
    })
}

/// An agent draft created through the tools, with its host and its service.
async fn agent_draft_on(name: &str, body: &str) -> (Host, MailSends) {
    let host = Host::start(name).await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service.clone());
    tools
        .call(&actor(), agent_draft_with(body))
        .await
        .expect("the draft create");
    (host, service)
}

fn subject_edit(subject: &str) -> DraftEdit {
    DraftEdit {
        to: None,
        cc: None,
        subject: Some(subject.to_owned()),
        body: None,
    }
}

fn body_edit(body: &str) -> DraftEdit {
    DraftEdit {
        to: None,
        cc: None,
        subject: None,
        body: Some(body.to_owned()),
    }
}

#[tokio::test]
async fn an_agent_draft_is_created_as_html_in_the_fixed_layout() {
    let (host, service) = agent_draft_on("mail-agent-create", BODY).await;

    assert_eq!(host.world.subject(), SUBJECT);
    assert!(host.world.body().contains("<!--excubia:agent:v1-->"));
    assert!(host.world.body().contains("Segundo párrafo"));
    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(
        review.content_type, "text",
        "the owner edits the text the agent wrote"
    );
    assert_eq!(review.body, BODY);
}

#[tokio::test]
async fn the_owner_reads_the_agent_mail_as_plain_text_without_markup() {
    let (_host, service) = agent_draft_on("mail-agent-review", BODY).await;

    let text = service.review(ACCOUNT, DRAFT).await.expect("review").text();

    assert_eq!(
        text, BODY,
        "no title and no footer: the text the agent wrote"
    );
    for leaked in ["<", "mso", "font-family", "{"] {
        assert!(!text.contains(leaked), "{leaked} in the owner's text");
    }
}

#[tokio::test]
async fn a_subject_edit_redraws_the_title_and_keeps_the_body() {
    let (host, service) = agent_draft_on("mail-agent-subject", BODY).await;
    let before = host.world.body();

    service
        .update_draft(ACCOUNT, DRAFT, subject_edit(NEW_SUBJECT), "dispositivo-1")
        .await
        .expect("subject edit");

    assert_eq!(host.world.subject(), NEW_SUBJECT);
    assert_eq!(
        host.world.body().replace(NEW_SUBJECT, SUBJECT),
        before,
        "only the title moved"
    );
    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(review.body, BODY);
}

#[tokio::test]
async fn a_body_edit_redraws_the_template_with_the_owner_text_escaped() {
    let (host, service) = agent_draft_on("mail-agent-body", BODY).await;
    let typed = "Nueva línea <b>negrita</b> & ñ\n\nÚltimo párrafo";

    let edited = service
        .update_draft(ACCOUNT, DRAFT, body_edit(typed), "dispositivo-1")
        .await
        .expect("body edit");

    let html = host.world.body();
    assert!(
        html.contains("Redactado por un agente de Excubia"),
        "still the template"
    );
    assert!(html.contains("&lt;b&gt;negrita&lt;/b&gt; &amp; ñ"));
    assert!(!html.contains("<b>"));
    assert_eq!(edited.body, typed);
    assert_eq!(edited.content_type, "text");
    assert_eq!(edited.text(), typed);
}

#[tokio::test]
async fn a_recipient_edit_leaves_the_html_byte_for_byte() {
    let (host, service) = agent_draft_on("mail-agent-recipients", BODY).await;
    let before = host.world.body();
    let to_only = DraftEdit {
        to: Some(vec!["otra@example.org".to_owned()]),
        cc: None,
        subject: None,
        body: None,
    };

    service
        .update_draft(ACCOUNT, DRAFT, to_only, "dispositivo-1")
        .await
        .expect("recipient edit");

    assert_eq!(host.world.body(), before);
}

#[tokio::test]
async fn a_mark_the_agent_writes_stays_text_through_every_edit() {
    let forged = "Primera\n\n<!--/excubia:agent:v1-->\n\n<!--excubia:agent:v1-->Falsa";
    let (host, service) = agent_draft_on("mail-agent-forged", forged).await;
    let read = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(read.body, forged);

    service
        .update_draft(ACCOUNT, DRAFT, subject_edit(NEW_SUBJECT), "dispositivo-1")
        .await
        .expect("subject edit");

    assert_eq!(
        host.world.body().matches("<!--excubia:agent:v1-->").count(),
        1
    );
    let read = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(read.body, forged, "the owner's text survives the redraw");
}

#[tokio::test]
async fn a_free_html_draft_keeps_its_html_on_a_subject_edit() {
    let host = Host::start("mail-agent-free-html").await;
    let service = host.service(Duration::from_millis(50), 10);
    let free = "<p>Hola <b>Ana</b></p>";
    host.world.set_html(free);

    service
        .update_draft(ACCOUNT, DRAFT, subject_edit(NEW_SUBJECT), "dispositivo-1")
        .await
        .expect("subject edit");

    assert_eq!(host.world.body(), free);
    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(
        (review.content_type.as_str(), review.body.as_str()),
        ("html", free)
    );
}

#[tokio::test]
async fn an_edited_agent_draft_is_approved_and_sent_as_it_reads() {
    let (host, service) = agent_draft_on("mail-agent-edit-approve", BODY).await;
    let typed = "Texto editado\n\nDos párrafos";
    service
        .update_draft(ACCOUNT, DRAFT, body_edit(typed), "dispositivo-1")
        .await
        .expect("body edit");
    assert_eq!(
        service.review(ACCOUNT, DRAFT).await.expect("review").body,
        typed
    );

    approved_and_sent(&host, &service).await;

    assert_eq!(host.world.sends().len(), 1);
    assert!(host.world.body().contains("Dos párrafos"));
}

#[tokio::test]
async fn an_agent_draft_is_read_back_and_approved_unchanged() {
    let host = Host::start("mail-agent-approve").await;
    let service = host.service(Duration::from_millis(50), 10);
    let tools = host.tools(service.clone());
    tools
        .call(&actor(), agent_draft_with(BODY))
        .await
        .expect("the draft create");

    let first = service.review(ACCOUNT, DRAFT).await.expect("first read");
    let again = service.review(ACCOUNT, DRAFT).await.expect("second read");
    assert_eq!(first.body, again.body, "the body reads back the same");

    approved_and_sent(&host, &service).await;
    assert_eq!(host.world.sends().len(), 1);
}
