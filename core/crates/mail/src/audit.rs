//! The local record of every file attached by path: the account, the draft,
//! the canonical path, size and SHA-256, and what was decided. The fields are
//! fixed here, so content, the upload URL and tokens have nowhere to go.
//!
//! One JSON object per line, appended with a single write, in the accounts
//! directory. Owner-only where the platform supports it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::MailError;
use crate::paths::CheckedFile;
use crate::private::open_append;
use crate::redact::Redacted;

const FILE_NAME: &str = "audit.jsonl";

/// What the policy decided about the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Inside the workspaces root, or approved before.
    Auto,
    /// A file the user sent in the chat: the user chose it, so nobody was asked.
    Chat,
    Approved,
    Denied,
    /// The file no longer matched the approval; nothing was sent.
    Changed,
    /// The question went unanswered until its deadline; nothing was sent. Kept
    /// apart from `denied`, because a refusal and a timeout are different facts.
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditRecord {
    /// Unix seconds.
    pub ts: u64,
    pub account: String,
    pub draft_id: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub decision: Decision,
}

/// How a send attempt ended. `Expired` and `Denied` are the operator's silence
/// and refusal; `Stale` is a draft that changed under its approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SendResult {
    Sent,
    Cancelled,
    Failed,
    Expired,
    Denied,
    Stale,
}

/// One decision about a mail send (MAIL2A), in the same file as the attachment
/// records and told apart by `kind`. It holds no subject, body, address or token:
/// recipients appear only as the domain of each one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SendRecord {
    pub kind: &'static str,
    pub ts: u64,
    pub account: String,
    pub draft_id: String,
    pub digest: String,
    /// The device that approved the send, when a device answered.
    pub approver: Option<String>,
    pub result: SendResult,
    /// One domain per recipient of the draft: `to` first, then `cc`.
    pub recipient_domains: Vec<String>,
}

impl SendRecord {
    /// `recipient_domains` are the domains of the draft's recipients (see
    /// [`domain_of`]), so a record rebuilt from the ledger holds the same thing.
    pub fn new(
        account: &str,
        draft_id: &str,
        digest: &str,
        approver: Option<&str>,
        result: SendResult,
        recipient_domains: Vec<String>,
    ) -> Self {
        Self {
            kind: "send",
            ts: unix_now(),
            account: account.to_owned(),
            draft_id: draft_id.to_owned(),
            digest: digest.to_owned(),
            approver: approver.map(str::to_owned),
            result,
            recipient_domains,
        }
    }
}

/// The owner changed a draft by hand (MAILUX-HOST §B): who did it and when. It
/// never holds what the draft says, so the audit keeps no content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditRecord {
    pub kind: &'static str,
    pub ts: u64,
    pub account: String,
    pub draft_id: String,
    /// The device that made the edit.
    pub device: String,
}

impl EditRecord {
    pub fn new(account: &str, draft_id: &str, device: &str) -> Self {
        Self {
            kind: "draft_edit",
            ts: unix_now(),
            account: account.to_owned(),
            draft_id: draft_id.to_owned(),
            device: device.to_owned(),
        }
    }
}

/// A read of the mailbox (MAIL-LECTURA): the account, the tool and, for one message,
/// its id. It never holds a subject, a body, an address or a filter the model wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadRecord {
    pub kind: &'static str,
    pub ts: u64,
    pub account: String,
    pub tool: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

impl ReadRecord {
    pub fn new(account: &str, tool: &'static str, message_id: Option<&str>) -> Self {
        Self {
            kind: "read",
            ts: unix_now(),
            account: account.to_owned(),
            tool,
            message_id: message_id.map(str::to_owned),
        }
    }
}

/// A document Alfred made (DOC8): the tool and the attachment id, never the content
/// or the name the file was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateRecord {
    pub kind: &'static str,
    pub ts: u64,
    pub tool: &'static str,
    pub attachment_id: String,
}

impl CreateRecord {
    pub fn new(tool: &'static str, attachment_id: &str) -> Self {
        Self {
            kind: "create",
            ts: unix_now(),
            tool,
            attachment_id: attachment_id.to_owned(),
        }
    }
}

/// The domain of an address, lowercased; empty when there is no `@`.
pub fn domain_of(address: &str) -> String {
    address
        .rsplit_once('@')
        .map_or_else(String::new, |(_, domain)| domain.to_ascii_lowercase())
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl AuditRecord {
    pub fn new(account: &str, draft_id: &str, file: &CheckedFile, decision: Decision) -> Self {
        Self {
            ts: unix_now(),
            account: account.to_owned(),
            draft_id: draft_id.to_owned(),
            path: file.path.to_string_lossy().into_owned(),
            size: file.size,
            sha256: file.sha256.clone(),
            decision,
        }
    }
}

pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    pub fn new(accounts_dir: &Path) -> Self {
        Self {
            path: accounts_dir.join(FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one record as a line. Any serialisable record fits: the file
    /// holds the attachment and the send records alike.
    pub fn append<T: Serialize>(&self, record: &T) -> Result<(), MailError> {
        let mut line = serde_json::to_vec(record).map_err(audit_error)?;
        line.push(b'\n');
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(audit_error)?;
        }
        let mut file = open_append(&self.path).map_err(audit_error)?;
        file.write_all(&line).map_err(audit_error)
    }
}

fn audit_error(reason: impl std::fmt::Display) -> MailError {
    MailError::Store(Redacted::new(format!(
        "the audit log cannot be written: {reason}"
    )))
}
