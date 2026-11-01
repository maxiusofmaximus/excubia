//! Reading the inbox (MAIL-LECTURA) against a mock Graph: listing, reading one
//! message with its checks, and the rule that every request is a GET.

mod support;

use hn_core_mail::inbox::inbox_url;
use hn_core_mail::received::{AuthSummary, MAX_BODY_CHARS};
use hn_core_mail::{GraphClient, MailError};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::{in_memory_ledger, ACCESS_TOKEN, ACCOUNT};

/// A bank notice that announces a file, reply-to and return-path of other domains,
/// a failed SPF and DMARC, and a link whose text names another host.
const NOTICE: &str = r#"{"id":"AAMk-1","subject":"Factura","from":{"emailAddress":{"name":"Banco","address":"avisos@banco.es"}},"sender":{"emailAddress":{"name":"Banco","address":"avisos@banco.es"}},"replyTo":[{"emailAddress":{"name":"","address":"phish@otro.net"}}],"toRecipients":[{"emailAddress":{"name":"Ana","address":"ana@x.com"}}],"ccRecipients":[],"receivedDateTime":"2026-10-30T08:00:00Z","internetMessageId":"<1@banco.es>","hasAttachments":false,"body":{"contentType":"html","content":"<p>Adjunto la factura. <a href='https://phish.example/p'>banco.es</a></p>"},"internetMessageHeaders":[{"name":"Return-Path","value":"<bounce@spam.example>"},{"name":"Authentication-Results","value":"mx.x.com; spf=fail smtp.mailfrom=spam.example; dkim=none; dmarc=fail"},{"name":"X-Secret","value":"token"}]}"#;

const NO_ATTACHMENTS: &str = r#"{"value":[]}"#;

#[test]
fn the_listing_url_names_the_folder_order_filter_and_fields() {
    let url = inbox_url("sentitems", 20, true, None);
    assert!(url.contains("/me/mailFolders/sentitems/messages?$top=20&$orderby="));
    assert!(url.contains("&$filter=isRead%20eq%20false"));
    assert!(
        url.contains("&$select=id,subject,from,receivedDateTime,isRead,hasAttachments,bodyPreview")
    );
    assert!(!inbox_url("inbox", 10, false, None).contains("$filter"));
}

#[tokio::test]
async fn the_inbox_list_is_one_get_with_the_preview_cut_short() {
    let body = json!({"value": [{
        "id": "m1",
        "subject": "Hola",
        "from": {"emailAddress": {"name": "Ana", "address": "ana@x.com"}},
        "receivedDateTime": "2026-10-30T07:00:00Z",
        "isRead": false,
        "hasAttachments": true,
        "bodyPreview": "x".repeat(300),
    }]})
    .to_string();
    let transport = MockTransport::new(vec![MockResponse::ok(body)]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let items = client
        .list_inbox("inbox", 10, false, None)
        .await
        .expect("list");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].from_address, "ana@x.com");
    assert!(!items[0].is_read);
    assert_eq!(items[0].preview.chars().count(), 200);
    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0]
        .url
        .contains("/me/mailFolders/inbox/messages?$top=10"));
}

#[tokio::test]
async fn a_sender_filter_cannot_break_out_of_its_quotes() {
    let transport = MockTransport::new(vec![MockResponse::ok(r#"{"value":[]}"#)]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    client
        .list_inbox("junkemail", 5, true, Some("x' or isRead eq true"))
        .await
        .expect("list");

    let url = &transport.calls()[0].url;
    assert!(url.contains("/mailFolders/junkemail/messages"));
    assert!(!url.contains('\''), "a quote must be encoded: {url}");
    assert!(url.contains("x%27%27%20or%20isRead%20eq%20true"), "{url}");
}

#[tokio::test]
async fn a_message_is_read_with_its_checks_and_only_gets_are_sent() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(NOTICE),
        MockResponse::ok(NO_ATTACHMENTS),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let message = client.read_message("AAMk-1").await.expect("message");

    assert_eq!(message.subject, "Factura");
    assert_eq!(message.from_address, "avisos@banco.es");
    assert_eq!(message.reply_to, ["phish@otro.net"]);
    assert!(message.body_text.contains("Adjunto la factura"));
    assert!(!message.truncated);
    assert_eq!(
        message.checks.auth,
        AuthSummary {
            spf: "fail",
            dkim: "none",
            dmarc: "fail",
        }
    );
    assert_eq!(
        message.checks.return_path_domain.as_deref(),
        Some("spam.example")
    );
    assert!(message.checks.reply_to_differs);
    assert!(message.checks.return_path_differs);
    assert!(message.checks.claims_attachment_but_none);
    assert!(message.checks.link_host_differs);
    assert_eq!(message.links[0].host, "phish.example");
    assert_eq!(
        message.headers.return_path.as_deref(),
        Some("<bounce@spam.example>")
    );

    let calls = transport.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].url.contains("/me/messages/AAMk-1?$select="));
    assert!(calls[1]
        .url
        .contains("/me/messages/AAMk-1/attachments?$select=name,contentType,size,isInline"));
    assert!(calls.iter().all(|call| call.method == "GET"));
    assert!(calls.iter().all(|call| !call.url.contains("$value")));
}

#[tokio::test]
async fn an_announced_attachment_that_arrives_is_listed_by_its_metadata_only() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(NOTICE),
        MockResponse::ok(
            r#"{"value":[{"name":"factura.pdf","contentType":"application/pdf","size":2048,"isInline":false}]}"#,
        ),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let message = client.read_message("AAMk-1").await.expect("message");

    assert!(!message.checks.claims_attachment_but_none);
    assert_eq!(message.attachments[0].name, "factura.pdf");
    assert_eq!(message.attachments[0].size, 2048);
    assert!(transport.calls().iter().all(|call| call.method == "GET"));
}

#[tokio::test]
async fn a_message_that_is_not_in_the_mailbox_is_not_found() {
    let transport = MockTransport::new(vec![MockResponse::status(
        404,
        r#"{"error":{"code":"ErrorItemNotFound"}}"#,
    )]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client.read_message("missing").await.expect_err("not found");

    assert!(matches!(error, MailError::MessageNotFound));
    assert_eq!(error.code(), "mail_not_found");
}

#[tokio::test]
async fn a_long_body_is_cut_and_says_so() {
    let body = json!({
        "id": "m2",
        "body": {"contentType": "text", "content": "a".repeat(MAX_BODY_CHARS + 1)},
    })
    .to_string();
    let transport = MockTransport::new(vec![
        MockResponse::ok(body),
        MockResponse::ok(NO_ATTACHMENTS),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let message = client.read_message("m2").await.expect("message");

    assert!(message.truncated);
    assert_eq!(message.body_text.chars().count(), MAX_BODY_CHARS);
}

#[tokio::test]
async fn at_most_twenty_links_are_returned_but_all_of_them_are_checked() {
    let mut html = String::new();
    for _ in 0..24 {
        html.push_str("<a href='https://banco.es/x'>banco.es</a> ");
    }
    html.push_str("<a href='https://phish.example/y'>banco.es</a>");
    let body = json!({"id": "m3", "body": {"contentType": "html", "content": html}}).to_string();
    let transport = MockTransport::new(vec![
        MockResponse::ok(body),
        MockResponse::ok(NO_ATTACHMENTS),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let message = client.read_message("m3").await.expect("message");

    assert_eq!(message.links.len(), 20);
    assert!(message.checks.link_host_differs);
}
