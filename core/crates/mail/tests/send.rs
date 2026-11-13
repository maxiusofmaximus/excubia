//! The send side of the mail crate (MAIL2A) without a socket: what the owner is
//! asked to approve, which drafts an account may send, and what the digest binds.

mod support;

use excubia_mail::{DraftBody, DraftInput, GraphClient, SendAttachment, SendView};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::{attachments_body, draft_input, in_memory_ledger, sha256_hex, ACCESS_TOKEN};

const ACCOUNT: &str = "trabajo";
const DRAFT: &str = "AAMk-draft-1";
const BODY: &str = "Hola Ana, adjunto el presupuesto.";

/// The draft the product creates for `body`, as the tools build it.
fn text_input(body: &str) -> DraftInput {
    DraftInput {
        to: vec!["ana@example.org".to_owned()],
        cc: Vec::new(),
        subject: "Presupuesto".to_owned(),
        body: DraftBody::Text(body.to_owned()),
    }
}

/// A Graph message as the draft endpoints return it, with the body that the
/// send is asked to approve.
fn send_message(id: &str, body: &str) -> String {
    send_message_as(id, "text", body)
}

fn send_message_as(id: &str, content_type: &str, body: &str) -> String {
    json!({
        "id": id,
        "subject": "Presupuesto",
        "from": {"emailAddress": {"address": "owner@example.com"}},
        "toRecipients": [{"emailAddress": {"address": "ana@example.org"}}],
        "ccRecipients": [],
        "isDraft": true,
        "hasAttachments": false,
        "body": {"contentType": content_type, "content": body},
    })
    .to_string()
}

fn view() -> SendView {
    SendView {
        account: ACCOUNT.to_owned(),
        draft_id: DRAFT.to_owned(),
        from: "owner@example.com".to_owned(),
        to: vec!["ana@example.org".to_owned()],
        cc: Vec::new(),
        subject: "Presupuesto".to_owned(),
        body_sha256: sha256_hex(BODY.as_bytes()),
        body_preview: "Hola Ana".to_owned(),
        attachments: vec![SendAttachment {
            name: "plan.pdf".to_owned(),
            size: 10,
            sha256: Some("ab".to_owned()),
        }],
    }
}

#[test]
fn the_digest_moves_with_one_change_in_any_covered_field() {
    let digest = view().digest();
    assert!(digest.starts_with("sha256:"), "{digest}");
    assert_eq!(
        view().digest(),
        digest,
        "the same view must give the same digest"
    );

    let mut subject = view();
    subject.subject.push('.');
    let mut recipient = view();
    recipient.to[0] = "ana@example.orc".to_owned();
    let mut copy = view();
    copy.cc.push("luis@example.org".to_owned());
    let mut sender = view();
    sender.from = "otro@example.com".to_owned();
    let mut body = view();
    body.body_sha256 = sha256_hex(b"Hola Ana, adjunto el presupuesto!");
    let mut attachment = view();
    attachment.attachments[0].sha256 = Some("ac".to_owned());

    for changed in [subject, recipient, copy, sender, body, attachment] {
        assert_ne!(
            changed.digest(),
            digest,
            "a changed view kept the digest: {changed:?}"
        );
    }
}

#[tokio::test]
async fn read_for_send_covers_the_body_by_digest_and_reads_with_the_login_token() {
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message(DRAFT, BODY)),
        MockResponse::ok(send_message(DRAFT, BODY)),
        MockResponse::ok(attachments_body(&[])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let meta = client.create_draft(&draft_input()).await.expect("draft");

    let read = client.read_for_send(&meta.id).await.expect("the send view");

    assert_eq!(read.body_sha256, sha256_hex(BODY.as_bytes()));
    assert_eq!(read.from, "owner@example.com");
    assert_eq!(read.to, ["ana@example.org"]);
    assert!(read.attachments.is_empty());
    let calls = transport.calls();
    assert_eq!(
        calls.len(),
        3,
        "create, read the message, list the attachments"
    );
    assert!(calls
        .iter()
        .all(|call| call.method != "POST" || call.url.ends_with("/me/messages")));
    assert!(calls
        .iter()
        .all(|call| call.bearer.as_deref() == Some(ACCESS_TOKEN)));
}

#[tokio::test]
async fn the_digest_covers_the_html_not_only_the_text_the_owner_reads() {
    let plain = "<div>Hola Ana</div>";
    let hidden = "<div>Hola <span style=\"display:none\"></span>Ana</div>";
    let mut digests = Vec::new();
    for html in [plain, hidden] {
        let transport = MockTransport::new(vec![
            MockResponse::created(send_message_as(DRAFT, "html", html)),
            MockResponse::ok(send_message_as(DRAFT, "html", html)),
            MockResponse::ok(attachments_body(&[])),
        ]);
        let ledger = in_memory_ledger();
        let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
        client.create_draft(&draft_input()).await.expect("draft");

        let view = client.read_for_send(DRAFT).await.expect("the send view");

        assert_eq!(view.body_preview, "Hola Ana");
        assert_eq!(view.body_sha256, sha256_hex(html.as_bytes()));
        digests.push(view.digest());
    }
    assert_ne!(
        digests[0], digests[1],
        "markup that the owner cannot see must change the approval"
    );
}

#[tokio::test]
async fn the_review_and_the_preview_show_the_text_not_the_markup() {
    let html = "<div>Hola Ana</div><div><br></div><div>Adjunto &amp; más</div>";
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message_as(DRAFT, "html", html)),
        MockResponse::ok(send_message_as(DRAFT, "html", html)),
        MockResponse::ok(attachments_body(&[])),
        MockResponse::ok(send_message_as(DRAFT, "html", html)),
        MockResponse::ok(attachments_body(&[])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client.create_draft(&draft_input()).await.expect("draft");
    let text = "Hola Ana\n\nAdjunto & más";

    let review = client.read_for_review(DRAFT).await.expect("the review");

    assert_eq!(review.content_type, "html");
    assert_eq!(review.body, html, "the review keeps what Graph stores");
    assert_eq!(review.text(), text);
    let view = client.read_for_send(DRAFT).await.expect("the send view");
    assert_eq!(view.body_preview, text);
}

#[tokio::test]
async fn an_ascii_body_padded_on_send_is_read_back_as_the_owner_wrote_it() {
    let stored = format!("{BODY}\u{a0}");
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message(DRAFT, &stored)),
        MockResponse::ok(send_message(DRAFT, &stored)),
        MockResponse::ok(attachments_body(&[])),
        MockResponse::ok(send_message(DRAFT, &stored)),
        MockResponse::ok(attachments_body(&[])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client.create_draft(&text_input(BODY)).await.expect("draft");

    let review = client.read_for_review(DRAFT).await.expect("the review");
    assert_eq!(review.body, BODY, "the added hard space is not the message");
    assert_eq!(review.text(), BODY);

    let view = client.read_for_send(DRAFT).await.expect("the send view");
    assert_eq!(view.body_preview, BODY);
    assert_eq!(
        view.body_sha256,
        sha256_hex(BODY.as_bytes()),
        "the digest binds the owner's body, not the padding"
    );
}

#[tokio::test]
async fn a_hard_space_the_owner_wrote_at_the_end_survives_the_read() {
    let owner_body = format!("{BODY} Álvarez\u{a0}");
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message(DRAFT, &owner_body)),
        MockResponse::ok(send_message(DRAFT, &owner_body)),
        MockResponse::ok(attachments_body(&[])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&text_input(&owner_body))
        .await
        .expect("draft");

    let review = client.read_for_review(DRAFT).await.expect("the review");

    assert_eq!(review.body, owner_body);
    assert_eq!(review.text(), owner_body);
}

#[tokio::test]
async fn an_owner_hard_space_after_ascii_text_is_not_taken_for_the_padding() {
    let owner_body = format!("{BODY}\u{a0}");
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message(DRAFT, &owner_body)),
        MockResponse::ok(send_message(DRAFT, &owner_body)),
        MockResponse::ok(attachments_body(&[])),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client
        .create_draft(&text_input(&owner_body))
        .await
        .expect("draft");
    assert_eq!(
        transport.calls()[0].json_body()["body"]["content"],
        owner_body,
        "a body with a hard space is not ASCII and goes as written"
    );

    let review = client.read_for_review(DRAFT).await.expect("the review");

    assert_eq!(
        review.body, owner_body,
        "only the hard space this host added is dropped"
    );
    assert_eq!(review.text(), owner_body);
}

#[tokio::test]
async fn a_draft_this_account_did_not_make_is_refused_before_graph_is_asked() {
    let transport = MockTransport::new(Vec::new());
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let read = client
        .read_for_send("someone-elses")
        .await
        .expect_err("not this account's draft");
    assert_eq!(read.code(), "mail_unknown_draft");
    let sent = client
        .send_draft("someone-elses")
        .await
        .expect_err("not this account's draft");
    assert_eq!(sent.code(), "mail_unknown_draft");

    assert!(
        transport.calls().is_empty(),
        "Graph was asked about a draft it does not own"
    );
}

#[tokio::test]
async fn a_draft_belongs_to_the_account_that_made_it() {
    let transport = MockTransport::new(vec![MockResponse::created(send_message(DRAFT, BODY))]);
    let ledger = in_memory_ledger();
    let owner = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    owner.create_draft(&draft_input()).await.expect("draft");
    let other = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, "otra");

    let refused = other
        .send_draft(DRAFT)
        .await
        .expect_err("another account must not send this draft");

    assert_eq!(refused.code(), "mail_unknown_draft");
    assert_eq!(transport.calls().len(), 1, "only the create reached Graph");
}

#[tokio::test]
async fn send_draft_posts_once_to_the_send_endpoint_with_the_token_it_was_given() {
    let transport = MockTransport::new(vec![
        MockResponse::created(send_message(DRAFT, BODY)),
        MockResponse::status(202, ""),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client.create_draft(&draft_input()).await.expect("draft");

    client.send_draft(DRAFT).await.expect("sent");

    let calls = transport.calls();
    let sent = calls.last().expect("the send call");
    assert_eq!(sent.method, "POST");
    assert!(
        sent.url.ends_with(&format!("/me/messages/{DRAFT}/send")),
        "{}",
        sent.url
    );
    assert_eq!(sent.bearer.as_deref(), Some(ACCESS_TOKEN));
    assert_eq!(calls.len(), 2, "nothing but the create and the send");
}
