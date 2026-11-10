mod support;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_core_mail::graph::fetch_upn;
use excubia_core_mail::{
    DraftBody, DraftInput, GraphClient, MailError, SmallAttachment, SMALL_ATTACHMENT_LIMIT,
};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::{attachments_body, in_memory_ledger, message_body, ACCESS_TOKEN, ACCOUNT};

const DRAFT_ID: &str = "draft-1";

fn input(subject: &str, body: DraftBody) -> DraftInput {
    DraftInput {
        to: vec!["ana@example.com".to_owned()],
        cc: vec!["luis@example.com".to_owned()],
        subject: subject.to_owned(),
        body,
    }
}

#[tokio::test]
async fn create_draft_sends_recipients_and_an_html_body() {
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let meta = client
        .create_draft(&input("Hola", DraftBody::Html("<p>Hola</p>".to_owned())))
        .await
        .expect("draft");

    assert_eq!(meta.id, DRAFT_ID);
    assert_eq!(meta.subject, "Hola");
    assert_eq!(meta.to, ["ana@example.com"]);
    assert_eq!(meta.cc, ["luis@example.com"]);
    assert!(meta.is_draft);
    assert!(meta.attachment_names.is_empty());

    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].url, "https://graph.microsoft.com/v1.0/me/messages");
    assert_eq!(calls[0].bearer.as_deref(), Some(ACCESS_TOKEN));
    let body = calls[0].json_body();
    assert_eq!(body["subject"], "Hola");
    assert_eq!(body["body"]["contentType"], "html");
    assert_eq!(body["body"]["content"], "<p>Hola</p>");
    assert_eq!(
        body["toRecipients"][0]["emailAddress"]["address"],
        "ana@example.com"
    );
    assert_eq!(
        body["ccRecipients"][0]["emailAddress"]["address"],
        "luis@example.com"
    );
}

#[tokio::test]
async fn create_draft_sends_a_text_body_as_text() {
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Texto", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Texto", DraftBody::Text("Hola".to_owned())))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["body"]["contentType"], "text");
    assert_eq!(body["body"]["content"], "Hola\u{a0}");
}

#[tokio::test]
async fn an_ascii_body_gets_one_hard_space_even_when_it_ends_in_a_space() {
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Text("Hola ".to_owned())))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["body"]["content"], "Hola \u{a0}");
    assert!(body["body"]["content"]
        .as_str()
        .expect("content")
        .ends_with('\u{a0}'));
}

#[tokio::test]
async fn a_body_with_one_non_ascii_character_keeps_its_bytes() {
    let text = "Hola Á";
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Text(text.to_owned())))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["body"]["content"], text);
}

#[tokio::test]
async fn an_html_body_is_never_padded() {
    let html = "<p>Hola</p>";
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Html(html.to_owned())))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["body"]["contentType"], "html");
    assert_eq!(body["body"]["content"], html);
}

#[tokio::test]
async fn a_text_draft_with_accents_keeps_its_subject_and_its_utf8_body() {
    let subject = "Á ñ ü €";
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, subject, false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input(
            subject,
            DraftBody::Text("Adjunto: Á ñ ü €".to_owned()),
        ))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["subject"], subject);
    assert_eq!(body["body"]["contentType"], "text");
    assert_eq!(body["body"]["content"], "Adjunto: Á ñ ü €");
}

#[tokio::test]
async fn an_explicit_html_body_is_sent_exactly_as_given() {
    let html = "<p>Ana &amp; Luis &lt;3</p>";
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Html(html.to_owned())))
        .await
        .expect("draft");
    let body = transport.calls()[0].json_body();
    assert_eq!(body["body"]["contentType"], "html");
    assert_eq!(body["body"]["content"], html);
}

#[tokio::test]
async fn the_same_content_within_fifteen_minutes_returns_the_same_draft() {
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(message_body("draft-2", "Otro", false)),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let draft = input("Hola", DraftBody::Text("Hola".to_owned()));

    let first = client.create_draft(&draft).await.expect("first");
    let second = client.create_draft(&draft).await.expect("second");
    assert_eq!(first.id, second.id);
    let calls = transport.calls();
    assert_eq!(
        calls.len(),
        2,
        "the repeat reads the draft, it never creates one"
    );
    assert!(calls[1].url.contains(DRAFT_ID), "{}", calls[1].url);

    let other = client
        .create_draft(&input("Otro", DraftBody::Text("Hola".to_owned())))
        .await
        .expect("other");
    assert_eq!(other.id, "draft-2");
    assert_eq!(transport.calls().len(), 3);
}

#[tokio::test]
async fn update_draft_patches_the_owned_draft() {
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Original", false)),
        MockResponse::ok(message_body(DRAFT_ID, "Nuevo", false)),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Original", DraftBody::Text("uno".to_owned())))
        .await
        .expect("create");
    let meta = client
        .update_draft(DRAFT_ID, &input("Nuevo", DraftBody::Text("dos".to_owned())))
        .await
        .expect("update");

    assert_eq!(meta.subject, "Nuevo");
    let calls = transport.calls();
    assert_eq!(calls[1].method, "PATCH");
    assert_eq!(
        calls[1].url,
        "https://graph.microsoft.com/v1.0/me/messages/draft-1"
    );
    assert_eq!(calls[1].json_body()["subject"], "Nuevo");
    assert_eq!(calls[1].json_body()["body"]["content"], "dos\u{a0}");
}

#[tokio::test]
async fn an_edit_with_an_ascii_body_applies_the_same_hard_space() {
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Original", false)),
        MockResponse::ok(message_body(DRAFT_ID, "Nuevo", false)),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Original", DraftBody::Text("uno".to_owned())))
        .await
        .expect("create");
    client
        .update_draft(
            DRAFT_ID,
            &input("Nuevo", DraftBody::Text("texto plano".to_owned())),
        )
        .await
        .expect("update");

    let calls = transport.calls();
    assert_eq!(calls[1].method, "PATCH");
    assert_eq!(calls[1].json_body()["body"]["content"], "texto plano\u{a0}");
}

#[tokio::test]
async fn get_draft_meta_reads_the_message_and_the_attachment_names() {
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(message_body(DRAFT_ID, "Hola", true)),
        MockResponse::ok(attachments_body(&["informe.pdf"])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Text("Hola".to_owned())))
        .await
        .expect("create");
    let meta = client.get_draft_meta(DRAFT_ID).await.expect("meta");

    assert_eq!(meta.attachment_names, ["informe.pdf"]);
    let calls = transport.calls();
    assert_eq!(calls[1].method, "GET");
    assert!(calls[1].url.contains("/me/messages/draft-1?$select="));
    assert!(calls[1].url.contains("hasAttachments"));
    assert_eq!(
        calls[2].url,
        "https://graph.microsoft.com/v1.0/me/messages/draft-1/attachments?$select=name"
    );
}

#[tokio::test]
async fn a_small_attachment_is_sent_as_base64() {
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(
            json!({"id": "att-1", "name": "informe.pdf", "size": 10, "contentType": "application/pdf"})
                .to_string(),
        ),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&input("Hola", DraftBody::Text("Hola".to_owned())))
        .await
        .expect("create");
    let attachment = SmallAttachment {
        file_name: "informe.pdf",
        content_type: "application/pdf",
        bytes: b"hola mundo",
    };
    let meta = client
        .add_small_attachment(DRAFT_ID, &attachment)
        .await
        .expect("attachment");

    assert_eq!(meta.name, "informe.pdf");
    assert_eq!(meta.size, 10);
    let call = &transport.calls()[1];
    assert_eq!(
        call.url,
        "https://graph.microsoft.com/v1.0/me/messages/draft-1/attachments"
    );
    let body = call.json_body();
    assert_eq!(body["@odata.type"], "#microsoft.graph.fileAttachment");
    assert_eq!(body["name"], "informe.pdf");
    assert_eq!(body["contentType"], "application/pdf");
    let decoded = STANDARD
        .decode(body["contentBytes"].as_str().expect("contentBytes"))
        .expect("base64");
    assert_eq!(decoded, b"hola mundo");
}

#[tokio::test]
async fn an_attachment_of_three_mib_needs_an_upload_session() {
    let transport = MockTransport::new(vec![]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let bytes = vec![0u8; SMALL_ATTACHMENT_LIMIT];
    let attachment = SmallAttachment {
        file_name: "grande.zip",
        content_type: "application/zip",
        bytes: &bytes,
    };
    let error = client
        .add_small_attachment(DRAFT_ID, &attachment)
        .await
        .expect_err("too large");
    match &error {
        MailError::UseUploadSession { size } => {
            assert_eq!(*size, SMALL_ATTACHMENT_LIMIT as u64);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(error.code(), "mail_use_upload_session");
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn a_draft_created_outside_this_client_is_never_touched() {
    let transport = MockTransport::new(vec![]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let error = client
        .get_draft_meta("draft-ajeno")
        .await
        .expect_err("foreign draft");
    assert!(matches!(error, MailError::DraftNotOwned { .. }));
    assert_eq!(error.code(), "mail_unknown_draft");
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn an_invalid_recipient_is_refused_before_any_request() {
    let transport = MockTransport::new(vec![]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let invalid = DraftInput {
        to: vec!["no es una direccion".to_owned()],
        cc: Vec::new(),
        subject: "Hola".to_owned(),
        body: DraftBody::Text("Hola".to_owned()),
    };
    let error = client.create_draft(&invalid).await.expect_err("invalid");
    assert!(matches!(error, MailError::InvalidInput(_)));
    assert_eq!(error.code(), "mail_invalid_input");
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn graph_errors_are_typed_and_redacted() {
    let transport = MockTransport::new(vec![MockResponse::status(
        400,
        json!({
            "error": {
                "code": "ErrorInvalidRecipients",
                "message": format!("one recipient is invalid: access_token={ACCESS_TOKEN}")
            }
        })
        .to_string(),
    )]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let error = client
        .create_draft(&input("Hola", DraftBody::Text("Hola".to_owned())))
        .await
        .expect_err("graph error");
    assert!(matches!(error, MailError::Graph { status: 400, .. }));
    assert_eq!(error.code(), "mail_graph_error");
    assert!(!format!("{error}").contains(ACCESS_TOKEN));
    assert!(!format!("{error:?}").contains(ACCESS_TOKEN));
}

#[tokio::test]
async fn a_rejected_access_token_asks_for_a_refresh() {
    let transport = MockTransport::new(vec![MockResponse::status(401, "{}")]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let error = client
        .create_draft(&input("Hola", DraftBody::Text("Hola".to_owned())))
        .await
        .expect_err("unauthorized");
    assert!(matches!(error, MailError::AuthRequired));
    assert_eq!(error.code(), "mail_auth_required");
}

#[tokio::test]
async fn the_sign_in_name_is_read_once_from_me() {
    let transport = MockTransport::new(vec![MockResponse::ok(
        json!({"userPrincipalName": "ana@example.com"}).to_string(),
    )]);

    let upn = fetch_upn(&transport, ACCESS_TOKEN).await.expect("upn");

    assert_eq!(upn, "ana@example.com");
    let calls = transport.calls();
    assert_eq!(
        calls[0].url,
        "https://graph.microsoft.com/v1.0/me?$select=userPrincipalName"
    );
    assert_eq!(calls[0].bearer.as_deref(), Some(ACCESS_TOKEN));
}
