mod support;

use std::fs;
use std::path::Path;

use hn_core_mail::{
    attach_chat_file, attach_file, Approval, AttachContext, AttachedFile, GraphClient, Ledger,
    MailError, SMALL_ATTACHMENT_LIMIT,
};
use support::mock::{MockResponse, MockTransport};
use support::{
    attachment_item, draft_input, in_memory_ledger, message_body, session_body, sha256_hex,
    sparse_file, CountingApproval, Harness, ACCESS_TOKEN, ACCOUNT, DRAFT_ID, UPLOAD_URL,
};

const SECRET_CONTENT: &str = "CONTENIDO-SECRETO-NO-DEBE-AUDITARSE";

async fn attach_once(
    client: &GraphClient<'_>,
    h: &Harness,
    gate: &CountingApproval,
    file: &Path,
) -> Result<AttachedFile, MailError> {
    let policy = h.policy();
    let ctx = AttachContext {
        client,
        account: ACCOUNT,
        policy: &policy,
        approval: gate,
        audit: &h.audit,
        scratch: &h.scratch,
    };
    attach_file(&ctx, DRAFT_ID, file).await
}

async fn client_with_draft<'a>(
    transport: &'a MockTransport,
    ledger: &'a Ledger,
) -> GraphClient<'a> {
    let client = GraphClient::new(transport, ACCESS_TOKEN, ledger, ACCOUNT);
    client.create_draft(&draft_input()).await.expect("draft");
    client
}

fn audit_lines(h: &Harness) -> Vec<serde_json::Value> {
    fs::read_to_string(h.audit.path())
        .expect("audit file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("json line"))
        .collect()
}

#[tokio::test]
async fn a_small_file_goes_as_a_simple_attachment() {
    let h = Harness::new("attach-small");
    let file = h.root.join("ventas.xlsx");
    fs::write(&file, b"contenido de prueba").expect("file");
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(attachment_item("att-1", "ventas.xlsx", 19)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Denied);

    let attached = attach_once(&client, &h, &gate, &file)
        .await
        .expect("attached");
    assert_eq!(attached.attachment_id, "att-1");
    assert_eq!(attached.size, 19);
    assert_eq!(attached.sha256, sha256_hex(b"contenido de prueba"));
    assert_eq!(
        gate.asked(),
        0,
        "a file in the workspace is never asked about"
    );

    let calls = transport.calls();
    assert!(calls[1].url.ends_with("/attachments"), "{}", calls[1].url);
    assert_eq!(calls[1].json_body()["name"], "ventas.xlsx");
    assert_eq!(
        calls[1].json_body()["contentType"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
}

#[tokio::test]
async fn just_under_three_mib_is_still_a_simple_attachment() {
    let h = Harness::new("attach-under");
    let file = h.root.join("casi.bin");
    sparse_file(&file, SMALL_ATTACHMENT_LIMIT as u64 - 1);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(attachment_item(
            "att-2",
            "casi.bin",
            SMALL_ATTACHMENT_LIMIT as u64 - 1,
        )),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Denied);

    attach_once(&client, &h, &gate, &file)
        .await
        .expect("attached");
    let calls = transport.calls();
    assert!(calls
        .iter()
        .all(|call| !call.url.contains("createUploadSession")));
}

#[tokio::test(start_paused = true)]
async fn three_mib_goes_through_an_upload_session() {
    let h = Harness::new("attach-three");
    let size = SMALL_ATTACHMENT_LIMIT as u64;
    let file = h.root.join("tres.bin");
    sparse_file(&file, size);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::created(attachment_item("att-3", "tres.bin", size)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Denied);

    let attached = attach_once(&client, &h, &gate, &file)
        .await
        .expect("uploaded");
    assert_eq!(attached.attachment_id, "att-3");
    assert_eq!(attached.size, size);
    let calls = transport.calls();
    assert!(calls
        .iter()
        .any(|call| call.url.ends_with("/createUploadSession")));
    assert!(calls.iter().all(|call| !call.url.ends_with("/attachments")));
}

#[tokio::test]
async fn a_denied_file_uploads_nothing_and_is_audited_as_denied() {
    let h = Harness::new("attach-denied");
    let file = h.outside.join("informe.txt");
    fs::write(&file, SECRET_CONTENT).expect("file");
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Denied);

    let err = attach_once(&client, &h, &gate, &file)
        .await
        .expect_err("denied");
    assert_eq!(err.code(), "mail_approval_denied");
    assert_eq!(gate.asked(), 1);
    assert_eq!(
        transport.calls().len(),
        1,
        "only the draft itself was created"
    );

    let lines = audit_lines(&h);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "denied");
    let raw = fs::read_to_string(h.audit.path()).expect("audit file");
    assert!(
        !raw.contains(SECRET_CONTENT),
        "the audit must hold no content"
    );
}

#[tokio::test]
async fn an_expired_question_uploads_nothing_and_is_audited_as_expired() {
    let h = Harness::new("attach-expired");
    let file = h.outside.join("informe.txt");
    fs::write(&file, SECRET_CONTENT).expect("file");
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Expired);

    let err = attach_once(&client, &h, &gate, &file)
        .await
        .expect_err("expired");
    assert_eq!(err.code(), "mail_approval_denied");
    assert_eq!(
        transport.calls().len(),
        1,
        "only the draft itself was created"
    );

    let lines = audit_lines(&h);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "expired");
}

#[tokio::test]
async fn an_approved_outside_file_is_attached_once_and_audited_without_content() {
    let h = Harness::new("attach-idempotent");
    let file = h.outside.join("informe.txt");
    fs::write(&file, SECRET_CONTENT).expect("file");
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(attachment_item(
            "att-4",
            "informe.txt",
            SECRET_CONTENT.len() as u64,
        )),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Approved);

    let first = attach_once(&client, &h, &gate, &file)
        .await
        .expect("first attach");
    let second = attach_once(&client, &h, &gate, &file)
        .await
        .expect("repeat attach");
    assert_eq!(first, second, "a repeat returns the same attachment");
    assert_eq!(
        gate.asked(),
        1,
        "an attachment already sent is not asked about again"
    );
    assert_eq!(transport.calls().len(), 2, "the repeat sends nothing");

    let lines = audit_lines(&h);
    assert_eq!(lines.len(), 1, "the audit records the upload once");
    assert_eq!(lines[0]["decision"], "approved");
    assert_eq!(lines[0]["sha256"], sha256_hex(SECRET_CONTENT.as_bytes()));
    let mut keys: Vec<&str> = lines[0]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["account", "decision", "draft_id", "path", "sha256", "size", "ts"]
    );
    let raw = fs::read_to_string(h.audit.path()).expect("audit file");
    assert!(
        !raw.contains(SECRET_CONTENT),
        "the audit must hold no content"
    );
    assert!(
        !raw.contains("tempauth"),
        "the audit must hold no upload URL"
    );
}

#[tokio::test(start_paused = true)]
async fn the_audit_of_a_session_upload_holds_no_upload_url() {
    let h = Harness::new("attach-audit-session");
    let size = SMALL_ATTACHMENT_LIMIT as u64;
    let file = h.outside.join("grande.bin");
    sparse_file(&file, size);
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::ok(session_body(UPLOAD_URL)),
        MockResponse::created(attachment_item("att-5", "grande.bin", size)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Approved);

    attach_once(&client, &h, &gate, &file)
        .await
        .expect("uploaded");
    let raw = fs::read_to_string(h.audit.path()).expect("audit file");
    assert!(
        !raw.contains("SESIONSECRETA"),
        "the audit must hold no upload URL"
    );
    assert_eq!(audit_lines(&h)[0]["decision"], "approved");
}

#[tokio::test]
async fn a_file_from_the_chat_goes_under_the_users_name_and_is_audited_as_chat() {
    let h = Harness::new("attach-chat");
    let stored = h.outside.join("att_01JZ0000000000000000000000");
    fs::write(&stored, b"contenido de prueba").expect("file");
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, "Hola", false)),
        MockResponse::created(attachment_item("att-3", "Informe final.xlsx", 19)),
    ]);
    let ledger = in_memory_ledger();
    let client = client_with_draft(&transport, &ledger).await;
    let gate = CountingApproval::new(Approval::Denied);
    let policy = h.policy();
    let ctx = AttachContext {
        client: &client,
        account: ACCOUNT,
        policy: &policy,
        approval: &gate,
        audit: &h.audit,
        scratch: &h.scratch,
    };

    let attached = attach_chat_file(&ctx, DRAFT_ID, &stored, "Informe final.xlsx")
        .await
        .expect("attached");
    assert_eq!(attached.name, "Informe final.xlsx");
    assert_eq!(gate.asked(), 0, "a file the user sent is never asked about");
    let calls = transport.calls();
    assert_eq!(calls[1].json_body()["name"], "Informe final.xlsx");
    assert_eq!(audit_lines(&h).last().expect("a line")["decision"], "chat");
}
