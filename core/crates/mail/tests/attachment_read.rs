//! Reading one attachment of a received message (TASK-ATT1) against a mock Graph:
//! the declared size is checked before any content is asked for, only file
//! attachments are read, the real size is checked again, and only GETs are sent.

mod support;

use excubia_mail::{GraphClient, MailError, MAX_READ_BYTES};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::{in_memory_ledger, ACCESS_TOKEN, ACCOUNT};

const FILE: &str = "#microsoft.graph.fileAttachment";

fn meta(odata: &str, name: &str, size: u64) -> String {
    json!({"@odata.type": odata, "id": "AAMk-att", "name": name, "size": size}).to_string()
}

fn content(base64: &str) -> String {
    json!({"contentBytes": base64}).to_string()
}

#[tokio::test]
async fn a_file_attachment_is_read_with_two_gets_and_its_bytes_decoded() {
    // "%PDF-" in base64, and the declared size of five bytes.
    let transport = MockTransport::new(vec![
        MockResponse::ok(meta(FILE, "factura.pdf", 5)),
        MockResponse::ok(content("JVBERi0=")),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let file = client
        .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
        .await
        .expect("the file");

    assert_eq!(file.name, "factura.pdf");
    assert_eq!(file.bytes, b"%PDF-");
    let calls = transport.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|call| call.method == "GET"));
    assert!(calls[0]
        .url
        .contains("/me/messages/AAMk-1/attachments/AAMk-att?$select=id,name,size"));
    assert!(calls[1].url.ends_with("/attachments/AAMk-att"));
}

#[tokio::test]
async fn a_declared_size_over_the_limit_is_refused_before_any_content_is_asked() {
    let transport = MockTransport::new(vec![MockResponse::ok(meta(
        FILE,
        "enorme.pdf",
        MAX_READ_BYTES + 1,
    ))]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
        .await
        .expect_err("too large");

    assert!(matches!(error, MailError::AttachmentTooLarge));
    assert_eq!(error.code(), "mail_attachment_too_large");
    // The transport answered one request only: the content was never asked for.
    assert_eq!(transport.calls().len(), 1);
}

#[tokio::test]
async fn a_real_size_over_the_limit_is_refused_even_when_the_declared_size_lies() {
    // Declared as 4 bytes; the decoded content is 12 MiB.
    let big = vec![b'a'; (MAX_READ_BYTES + 1) as usize];
    let base64 = {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        STANDARD.encode(&big)
    };
    let transport = MockTransport::new(vec![
        MockResponse::ok(meta(FILE, "mentira.pdf", 4)),
        MockResponse::ok(content(&base64)),
    ]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
        .await
        .expect_err("the real size");

    assert_eq!(error.code(), "mail_attachment_too_large");
}

#[tokio::test]
async fn an_item_attachment_or_a_link_is_refused_with_its_own_error() {
    for odata in [
        "#microsoft.graph.itemAttachment",
        "#microsoft.graph.referenceAttachment",
    ] {
        let transport = MockTransport::new(vec![MockResponse::ok(meta(odata, "otro.msg", 10))]);
        let ledger = in_memory_ledger();
        let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

        let error = client
            .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
            .await
            .expect_err(odata);

        assert_eq!(error.code(), "mail_attachment_not_file", "{odata}");
        assert_eq!(transport.calls().len(), 1, "no content request for {odata}");
    }
}

#[tokio::test]
async fn a_missing_type_is_refused_as_not_a_file() {
    let transport = MockTransport::new(vec![MockResponse::ok(
        json!({"id": "AAMk-att", "name": "sin-tipo.pdf", "size": 5}).to_string(),
    )]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
        .await
        .expect_err("no type");

    assert_eq!(error.code(), "mail_attachment_not_file");
}

#[tokio::test]
async fn an_attachment_the_message_does_not_have_is_missing_and_the_ids_stay_out_of_the_error() {
    let transport = MockTransport::new(vec![MockResponse::status(
        404,
        r#"{"error":{"code":"ErrorItemNotFound","message":"no"}}"#,
    )]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .fetch_attachment("AAMk-1", "AAMk-att", MAX_READ_BYTES)
        .await
        .expect_err("missing");

    assert_eq!(error.code(), "mail_attachment_missing");
    let text = error.to_string();
    assert!(!text.contains("AAMk"), "{text}");
}
