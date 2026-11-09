mod support;

use std::path::Path;

use hn_core_mail::{
    attach_file, Approval, AttachContext, AttachedFile, GraphClient, Ledger, MailError,
    DEFAULT_UPLOAD_CHUNK_BYTES, MAX_UPLOAD_CHUNK_BYTES, UPLOAD_GRANULE_BYTES,
};
use support::mock::{Call, MockResponse, MockTransport};
use support::{
    attachment_item, attachments_body, draft_input, in_memory_ledger, message_body, pending_body, session_body,
    sparse_file, CountingApproval, Harness, ACCESS_TOKEN, ACCOUNT, DRAFT_ID, UPLOAD_URL,
};

const SEVEN_MIB: u64 = 7 * 1024 * 1024;
const CHUNK: u64 = DEFAULT_UPLOAD_CHUNK_BYTES as u64;

async fn client_with_draft<'a>(
    transport: &'a MockTransport,
    ledger: &'a Ledger,
) -> GraphClient<'a> {
    let client = GraphClient::new(transport, ACCESS_TOKEN, ledger, ACCOUNT);
    client.create_draft(&draft_input()).await.expect("draft");
    client
}

async fn attach_once(
    client: &GraphClient<'_>,
    h: &Harness,
    file: &Path,
) -> Result<AttachedFile, MailError> {
    let gate = CountingApproval::new(Approval::Denied);
    let policy = h.policy();
    let ctx = AttachContext {
        client,
        account: ACCOUNT,
        policy: &policy,
        approval: &gate,
        audit: &h.audit,
        scratch: &h.scratch,
    };
    attach_file(&ctx, DRAFT_ID, file).await
}

/// `bytes a-b/total` into its three numbers.
fn parse_range(header: &str) -> (u64, u64, u64) {
    let span_and_total = header.strip_prefix("bytes ").expect("a bytes range");
    let (span, total) = span_and_total.split_once('/').expect("a total");
    let (first, last) = span.split_once('-').expect("a span");
    (
        first.parse().expect("first byte"),
        last.parse().expect("last byte"),
        total.parse().expect("total"),
    )
}

fn puts(calls: &[Call]) -> Vec<&Call> {
    calls.iter().filter(|call| call.method == "PUT").collect()
}

#[test]
fn the_default_chunk_is_twelve_granules_and_within_the_ceiling() {
    assert_eq!(DEFAULT_UPLOAD_CHUNK_BYTES, 12 * 320 * 1024);
    assert_eq!(DEFAULT_UPLOAD_CHUNK_BYTES % UPLOAD_GRANULE_BYTES, 0);
    assert_eq!(
        DEFAULT_UPLOAD_CHUNK_BYTES.min(MAX_UPLOAD_CHUNK_BYTES),
        DEFAULT_UPLOAD_CHUNK_BYTES
    );
}

#[tokio::test(start_paused = true)]
async fn an_empty_final_answer_is_resolved_from_the_draft_attachments() {
    let h = Harness::new("upload-empty-final");
    let file = h.root.join("plano.xlsx");
    sparse_file(&file, SEVEN_MIB);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::status(202, pending_body(&format!("{CHUNK}-"))),
        MockResponse::created(""),
        MockResponse::ok(attachments_body(&["otro.pdf", "plano.xlsx"])),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;

    let attached = attach_once(&client, &h, &file).await.expect("uploaded");
    assert_eq!(attached.attachment_id, "att-plano.xlsx");
}

#[tokio::test(start_paused = true)]
async fn a_seven_mib_file_goes_in_contiguous_chunks_without_a_bearer() {
    let h = Harness::new("upload-seven");
    let file = h.root.join("plano.xlsx");
    sparse_file(&file, SEVEN_MIB);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::status(202, pending_body(&format!("{CHUNK}-"))),
        MockResponse::created(attachment_item("att-7", "plano.xlsx", SEVEN_MIB)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;

    let attached = attach_once(&client, &h, &file).await.expect("uploaded");
    assert_eq!(attached.attachment_id, "att-7");
    assert_eq!(attached.size, SEVEN_MIB);

    let calls = transport.calls();
    let session = calls
        .iter()
        .find(|call| call.url.ends_with("/createUploadSession"))
        .expect("a session was created");
    assert_eq!(
        session.json_body()["AttachmentItem"]["attachmentType"],
        "file"
    );
    assert_eq!(session.json_body()["AttachmentItem"]["name"], "plano.xlsx");
    assert_eq!(session.json_body()["AttachmentItem"]["size"], SEVEN_MIB);

    let chunks = puts(&calls);
    let spans: Vec<(u64, u64, u64)> = chunks
        .iter()
        .map(|call| parse_range(call.range.as_deref().expect("a range")))
        .collect();
    assert_eq!(
        spans,
        [(0, CHUNK - 1, SEVEN_MIB), (CHUNK, SEVEN_MIB - 1, SEVEN_MIB)]
    );

    let sizes: Vec<usize> = chunks.iter().map(|call| call.payload_len()).collect();
    assert_eq!(
        sizes,
        [DEFAULT_UPLOAD_CHUNK_BYTES, (SEVEN_MIB - CHUNK) as usize]
    );
    assert_eq!(sizes[0] % UPLOAD_GRANULE_BYTES, 0);
    for chunk in &chunks {
        assert_eq!(chunk.url, UPLOAD_URL);
        assert_eq!(
            chunk.bearer, None,
            "a chunk must never carry a bearer token"
        );
        assert!(chunk.payload_len() <= MAX_UPLOAD_CHUNK_BYTES);
    }
}

#[tokio::test(start_paused = true)]
async fn a_transient_failure_resumes_from_the_offset_graph_reports() {
    let h = Harness::new("upload-resume");
    let file = h.root.join("plano.xlsx");
    sparse_file(&file, SEVEN_MIB);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::status(202, pending_body(&format!("{CHUNK}-"))),
        MockResponse::status(503, "unavailable"),
        MockResponse::ok(pending_body(&format!("{CHUNK}-"))),
        MockResponse::created(attachment_item("att-8", "plano.xlsx", SEVEN_MIB)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;

    let attached = attach_once(&client, &h, &file).await.expect("uploaded");
    assert_eq!(attached.attachment_id, "att-8");

    let calls = transport.calls();
    let spans: Vec<(u64, u64, u64)> = puts(&calls)
        .iter()
        .map(|call| parse_range(call.range.as_deref().expect("a range")))
        .collect();
    assert_eq!(
        spans,
        [
            (0, CHUNK - 1, SEVEN_MIB),
            (CHUNK, SEVEN_MIB - 1, SEVEN_MIB),
            (CHUNK, SEVEN_MIB - 1, SEVEN_MIB)
        ],
        "the failed chunk is sent again from the offset the status reports"
    );

    let status: Vec<&Call> = calls.iter().filter(|call| call.method == "GET").collect();
    assert_eq!(status.len(), 1, "one status query after the failure");
    assert_eq!(status[0].url, UPLOAD_URL);
    assert_eq!(status[0].bearer, None);
}

#[tokio::test(start_paused = true)]
async fn a_fourth_failure_in_a_row_is_a_clear_error_that_hides_the_upload_url() {
    let h = Harness::new("upload-exhausted");
    let file = h.root.join("plano.xlsx");
    sparse_file(&file, SEVEN_MIB);
    let resume = pending_body(&format!("{CHUNK}-"));
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::status(202, pending_body(&format!("{CHUNK}-"))),
        MockResponse::status(503, "unavailable"),
        MockResponse::ok(resume.clone()),
        MockResponse::status(503, "unavailable"),
        MockResponse::ok(resume.clone()),
        MockResponse::status(503, "unavailable"),
        MockResponse::ok(resume),
        MockResponse::status(503, "unavailable"),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;

    let err = attach_once(&client, &h, &file)
        .await
        .expect_err("four failures");
    assert_eq!(err.code(), "mail_upload_failed");
    assert!(err.to_string().contains("failed 4 times in a row"), "{err}");
    let shown = format!("{err} {err:?}");
    assert!(
        !shown.contains("SESIONSECRETA"),
        "the upload URL leaked: {shown}"
    );
    assert!(
        !shown.contains("tempauth"),
        "the upload URL leaked: {shown}"
    );

    let calls = transport.calls();
    assert_eq!(
        puts(&calls).len(),
        5,
        "one chunk plus four attempts at the second"
    );
    assert_eq!(calls.iter().filter(|call| call.method == "GET").count(), 3);
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_stops_at_once() {
    let h = Harness::new("upload-expired");
    let file = h.root.join("plano.xlsx");
    sparse_file(&file, SEVEN_MIB);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::status(410, "expired"),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;

    let err = attach_once(&client, &h, &file).await.expect_err("expired");
    assert_eq!(err.code(), "mail_upload_failed");
    let calls = transport.calls();
    assert_eq!(puts(&calls).len(), 1, "no retry for an expired session");
    assert!(calls.iter().all(|call| call.method != "GET"));
}
