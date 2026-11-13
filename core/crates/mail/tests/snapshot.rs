//! The bytes sent are the bytes approved (MAIL1C, arreglo 0): a file that
//! changes between the approval and the copy is refused, a file that changes
//! after the copy sends the copy, and no copy outlives its attach.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_mail::{
    attach_file, Approval, ApprovalGate, AttachContext, AttachedFile, GraphClient, MailError,
    PathApproval,
};
use support::mock::{MockResponse, MockTransport};
use support::{
    attachment_item, draft_input, in_memory_ledger, message_body, Harness, ACCESS_TOKEN, ACCOUNT,
    DRAFT_ID,
};

const APPROVED: &[u8] = b"CONTENIDO APROBADO";
const CHANGED: &[u8] = b"CONTENIDO CAMBIADO DESPUES";

/// Rewrites the file when it is asked about, then approves: the change lands
/// between the approval and the copy.
struct RewritingApproval {
    path: PathBuf,
}

#[async_trait]
impl ApprovalGate for RewritingApproval {
    async fn decide(&self, _request: &PathApproval) -> Result<Approval, MailError> {
        fs::write(&self.path, CHANGED).expect("rewrite the file");
        Ok(Approval::Approved)
    }
}

/// Approves without touching anything.
struct PlainApproval;

#[async_trait]
impl ApprovalGate for PlainApproval {
    async fn decide(&self, _request: &PathApproval) -> Result<Approval, MailError> {
        Ok(Approval::Approved)
    }
}

async fn attach_with(
    transport: &MockTransport,
    h: &Harness,
    gate: &dyn ApprovalGate,
    file: &Path,
) -> Result<AttachedFile, MailError> {
    let ledger = in_memory_ledger();
    let client = GraphClient::new(transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    client.create_draft(&draft_input()).await.expect("draft");
    let policy = h.policy();
    let ctx = AttachContext {
        client: &client,
        account: ACCOUNT,
        policy: &policy,
        approval: gate,
        audit: &h.audit,
        scratch: &h.scratch,
    };
    attach_file(&ctx, DRAFT_ID, file).await
}

fn draft_answer() -> MockResponse {
    MockResponse::created(message_body(DRAFT_ID, "Hola", false))
}

fn last_audit_decision(h: &Harness) -> String {
    let text = fs::read_to_string(h.audit.path()).expect("audit file");
    let last: serde_json::Value =
        serde_json::from_str(text.lines().last().expect("a line")).expect("json");
    last["decision"].as_str().expect("decision").to_owned()
}

#[tokio::test]
async fn a_change_between_approval_and_copy_is_refused_and_nothing_is_sent() {
    let h = Harness::new("snapshot-changed-before-copy");
    let file = h.outside.join("informe.txt");
    fs::write(&file, APPROVED).expect("file");
    let transport = MockTransport::new(vec![draft_answer()]);
    let gate = RewritingApproval { path: file.clone() };

    let err = attach_with(&transport, &h, &gate, &file)
        .await
        .expect_err("the change must be refused");

    assert!(matches!(err, MailError::ContentChanged), "{err:?}");
    assert_eq!(err.code(), "mail_content_changed");
    assert_eq!(
        transport.calls().len(),
        1,
        "only the draft was created; no byte of the file left"
    );
    assert_eq!(last_audit_decision(&h), "changed");
    assert_eq!(h.scratch_entries(), 0, "the copy is removed");
}

#[tokio::test]
async fn the_upload_sends_the_approved_copy_even_if_the_file_changes_after_it() {
    let h = Harness::new("snapshot-changed-after-copy");
    let file = h.outside.join("informe.txt");
    fs::write(&file, APPROVED).expect("file");
    let tampered = file.clone();
    let transport = MockTransport::new(vec![
        draft_answer(),
        MockResponse::created(attachment_item("att-1", "informe.txt", 18)),
    ])
    .with_hook(move |call| {
        if call.url.ends_with("/attachments") {
            fs::write(&tampered, CHANGED).expect("tamper after the copy");
        }
    });

    let attached = attach_with(&transport, &h, &PlainApproval, &file)
        .await
        .expect("attached");

    assert_eq!(attached.attachment_id, "att-1");
    let calls = transport.calls();
    let sent = STANDARD
        .decode(
            calls[1].json_body()["contentBytes"]
                .as_str()
                .expect("bytes"),
        )
        .expect("base64");
    assert_eq!(sent, APPROVED, "the approved bytes, not the changed ones");
    assert_eq!(last_audit_decision(&h), "approved");
    assert_eq!(h.scratch_entries(), 0, "the copy is removed after success");
}

#[tokio::test]
async fn the_copy_is_removed_when_the_upload_fails_and_stays_out_of_the_audit() {
    let h = Harness::new("snapshot-upload-fails");
    let file = h.outside.join("informe.txt");
    fs::write(&file, APPROVED).expect("file");
    let transport = MockTransport::new(vec![
        draft_answer(),
        MockResponse::status(
            500,
            r#"{"error":{"code":"serviceNotAvailable","message":"no"}}"#,
        ),
    ]);

    let err = attach_with(&transport, &h, &PlainApproval, &file)
        .await
        .expect_err("the upload fails");

    assert!(
        matches!(err, MailError::Graph { status: 500, .. }),
        "{err:?}"
    );
    assert_eq!(h.scratch_entries(), 0, "the copy is removed after an error");
    let audit = fs::read_to_string(h.audit.path()).expect("audit file");
    assert!(
        !audit.contains(&h.scratch.to_string_lossy().to_string()),
        "the copy's location never reaches the audit"
    );
}
