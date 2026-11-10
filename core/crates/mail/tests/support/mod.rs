#![allow(dead_code)]

pub mod mock;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use excubia_core_mail::{
    Approval, ApprovalGate, AuditLog, DraftBody, DraftInput, Ledger, MailError, PathApproval,
    PathPolicy, DEFAULT_MAX_ATTACHMENT_BYTES,
};
use excubia_core_test_dir::TestDir;
use serde_json::json;
use sha2::{Digest, Sha256};

pub const CLIENT_ID: &str = "11111111-2222-3333-4444-555555555555";
pub const ACCESS_TOKEN: &str =
    "eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJleGN1YmlhIn0.c2lnbmF0dXJlLWxvbmctZW5vdWdo";
pub const REFRESH_TOKEN: &str =
    "M.R3_BAY.-C2x0Y2JjLTRhYjctNGQxMS1hYmNkLWVmMDEyMzQ1Njc4OTBhYmNkZWZnaGlqa2xtbm9wcXJzdHV2";
pub const DEVICE_CODE: &str =
    "EwBAA8l6BAAU6rZ9V0YxM2JjLTRhYjctNGQxMS1hYmNkLWVmMDEyMzQ1Njc4OTBhYmNkZWZnaGlqa2xtbm9wcXJzdHV2";

pub const DRAFT_ID: &str = "draft-1";
pub const ACCOUNT: &str = "trabajo";
/// A session URL with a secret in it. No error, record or log may show it.
pub const UPLOAD_URL: &str = "https://upload.example.invalid/session/SESIONSECRETA-0123456789abcdefghijklmnopqrstuvwxyz?tempauth=ZZZ";

pub fn device_code_body() -> String {
    json!({
        "device_code": DEVICE_CODE,
        "user_code": "ABCD-EFGH",
        "verification_uri": "https://microsoft.com/devicelogin",
        "expires_in": 900,
        "interval": 5,
        "message": "To sign in, use a web browser to open the page and enter the code",
    })
    .to_string()
}

pub fn token_body(access_token: &str, refresh_token: Option<&str>) -> String {
    let mut body = json!({
        "token_type": "Bearer",
        "access_token": access_token,
        "expires_in": 3600,
    });
    if let Some(refresh) = refresh_token {
        body["refresh_token"] = json!(refresh);
    }
    body.to_string()
}

pub fn message_body(id: &str, subject: &str, has_attachments: bool) -> String {
    json!({
        "id": id,
        "subject": subject,
        "isDraft": true,
        "hasAttachments": has_attachments,
        "toRecipients": [{"emailAddress": {"address": "ana@example.com"}}],
        "ccRecipients": [{"emailAddress": {"address": "luis@example.com"}}],
    })
    .to_string()
}

pub fn attachments_body(names: &[&str]) -> String {
    let value: Vec<serde_json::Value> = names
        .iter()
        .map(|name| json!({"id": format!("att-{name}"), "name": name, "size": 12}))
        .collect();
    json!({"value": value}).to_string()
}

/// The answer to `createUploadSession`.
pub fn session_body(upload_url: &str) -> String {
    json!({
        "uploadUrl": upload_url,
        "expirationDateTime": "2026-10-31T00:00:00Z",
        "nextExpectedRanges": ["0-"],
    })
    .to_string()
}

/// A session status, or a chunk answer that is not the last one.
pub fn pending_body(next_expected: &str) -> String {
    json!({
        "expirationDateTime": "2026-10-31T00:00:00Z",
        "nextExpectedRanges": [next_expected],
    })
    .to_string()
}

/// The attachment Graph returns once the bytes are all in.
pub fn attachment_item(id: &str, name: &str, size: u64) -> String {
    json!({
        "@odata.type": "#microsoft.graph.fileAttachment",
        "id": id,
        "name": name,
        "size": size,
    })
    .to_string()
}

pub fn draft_input() -> DraftInput {
    DraftInput {
        to: vec!["ana@example.com".to_owned()],
        cc: Vec::new(),
        subject: "Hola".to_owned(),
        body: DraftBody::Text("Adjunto".to_owned()),
    }
}

/// A ledger for one test, in memory: nothing outlives the test.
pub fn in_memory_ledger() -> Ledger {
    Ledger::in_memory().expect("ledger")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A file of `size` bytes that takes no disk space beyond its metadata.
pub fn sparse_file(path: &Path, size: u64) {
    File::create(path)
        .expect("create the file")
        .set_len(size)
        .expect("set its size");
}

/// A private folder for one test: a workspaces root, a folder outside it, and an audit log.
pub struct Harness {
    pub dir: TestDir,
    pub root: PathBuf,
    pub outside: PathBuf,
    pub audit: AuditLog,
    /// The private copies of approved files, as the accounts folder holds them.
    pub scratch: PathBuf,
}

impl Harness {
    pub fn new(label: &str) -> Self {
        let dir = TestDir::new(label);
        let root = dir.join("workspaces");
        let outside = dir.join("outside");
        fs::create_dir_all(&root).expect("workspace folder");
        fs::create_dir_all(&outside).expect("outside folder");
        let accounts = dir.join("accounts");
        let audit = AuditLog::new(&accounts);
        let scratch = accounts.join("tmp");
        Self {
            dir,
            root,
            outside,
            audit,
            scratch,
        }
    }

    /// The files in the scratch folder: a successful or failed attach leaves none.
    pub fn scratch_entries(&self) -> usize {
        fs::read_dir(&self.scratch).map_or(0, |entries| entries.count())
    }

    pub fn policy(&self) -> PathPolicy {
        PathPolicy::new(&self.root, DEFAULT_MAX_ATTACHMENT_BYTES).expect("policy")
    }
}

/// An approval gate with a fixed answer that counts how often it was asked.
pub struct CountingApproval {
    answer: Approval,
    asked: AtomicUsize,
}

impl CountingApproval {
    pub fn new(answer: Approval) -> Self {
        Self {
            answer,
            asked: AtomicUsize::new(0),
        }
    }

    pub fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ApprovalGate for CountingApproval {
    async fn decide(&self, _request: &PathApproval) -> Result<Approval, MailError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(self.answer)
    }
}
