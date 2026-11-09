//! Attaching a local file to a draft this client created: the entry point for
//! attachments by path.
//!
//! The order is the contract. The draft must be ours. The path must pass the
//! policy. An identical attachment is never sent twice. Only then is the
//! decision audited, before a byte leaves the machine, and the upload starts.

use std::path::Path;

use async_trait::async_trait;

use crate::audit::{AuditLog, AuditRecord, Decision};
use crate::draft::{AttachmentMeta, SmallAttachment};
use crate::error::MailError;
use crate::graph::{GraphClient, SMALL_ATTACHMENT_LIMIT};
use crate::paths::{unreadable, CheckedFile, PathApproval, PathDecision, PathPolicy};
use crate::redact::Redacted;
use crate::snapshot::Snapshot;

/// The user's answer to a request to attach a file from outside the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Approved,
    Denied,
    /// No answer before the deadline. Nothing was sent, as with a denial.
    Expired,
}

/// Asks the user. MAIL1C implements it over the UAC flow; tests use a fixed answer.
#[async_trait]
pub trait ApprovalGate: Send + Sync {
    /// `Err` means no answer was obtained, which is not a denial.
    async fn decide(&self, request: &PathApproval) -> Result<Approval, MailError>;
}

/// An attachment Graph accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedFile {
    pub attachment_id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

/// Everything [`attach_file`] needs besides the draft and the path.
pub struct AttachContext<'c, 'a> {
    pub client: &'c GraphClient<'a>,
    /// The label of the account that owns `client`, for the audit.
    pub account: &'c str,
    pub policy: &'c PathPolicy,
    pub approval: &'c dyn ApprovalGate,
    pub audit: &'c AuditLog,
    /// Where the private copy of an approved file waits for its upload.
    pub scratch: &'c Path,
}

/// Attaches `requested` to `draft_id`. The bytes sent are the bytes approved:
/// the upload reads a private copy, checked against the approval before the
/// decision is audited, and the copy is removed whatever the outcome.
pub async fn attach_file(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    requested: &Path,
) -> Result<AttachedFile, MailError> {
    ctx.client.owned(draft_id)?;
    let decision = ctx.policy.decide(requested)?;
    let file = decision.file().clone();
    let name = file
        .file_name()
        .ok_or_else(|| MailError::PathRejected(Redacted::new("the file has no name")))?
        .to_owned();
    if let Some(attached) = already_attached(ctx, draft_id, &file, &name)? {
        return Ok(attached);
    }

    let audited = match decision {
        PathDecision::Allowed(_) => Decision::Auto,
        PathDecision::NeedsApproval(pending) => match ctx.approval.decide(&pending).await? {
            Approval::Approved => Decision::Approved,
            Approval::Denied => return refuse(ctx, draft_id, &file, Decision::Denied),
            Approval::Expired => return refuse(ctx, draft_id, &file, Decision::Expired),
        },
    };
    send(ctx, draft_id, &file, &name, audited).await
}

/// Attaches a file the user sent in the chat, under the name the user gave it. The
/// host resolved `path` from its own store, so no approval is asked.
pub async fn attach_chat_file(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    path: &Path,
    name: &str,
) -> Result<AttachedFile, MailError> {
    ctx.client.owned(draft_id)?;
    let file = ctx.policy.decide_trusted(path)?;
    if let Some(attached) = already_attached(ctx, draft_id, &file, name)? {
        return Ok(attached);
    }
    send(ctx, draft_id, &file, name, Decision::Chat).await
}

/// The file is on the draft already, under this name: nothing is sent twice.
fn already_attached(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    file: &CheckedFile,
    name: &str,
) -> Result<Option<AttachedFile>, MailError> {
    let attached = ctx.client.attached(draft_id, &file.sha256, name)?;
    Ok(attached.map(|meta| attached_file(meta, file.sha256.clone())))
}

/// The private copy, checked against the decision, then the audit line, then the upload.
async fn send(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    file: &CheckedFile,
    name: &str,
    audited: Decision,
) -> Result<AttachedFile, MailError> {
    let snapshot = match Snapshot::take(file, ctx.scratch) {
        Ok(snapshot) => snapshot,
        Err(MailError::ContentChanged) => {
            record(ctx, draft_id, file, Decision::Changed)?;
            return Err(MailError::ContentChanged);
        }
        Err(err) => return Err(err),
    };
    record(ctx, draft_id, file, audited)?;

    let meta = upload(ctx.client, draft_id, name, snapshot.file()).await?;
    Ok(attached_file(meta, file.sha256.clone()))
}

fn record(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    file: &CheckedFile,
    decision: Decision,
) -> Result<(), MailError> {
    ctx.audit
        .append(&AuditRecord::new(ctx.account, draft_id, file, decision))
}

/// A file the user did not let through: audited as what happened, and reported
/// as a refusal, because nothing was sent in either case.
fn refuse(
    ctx: &AttachContext<'_, '_>,
    draft_id: &str,
    file: &CheckedFile,
    decision: Decision,
) -> Result<AttachedFile, MailError> {
    record(ctx, draft_id, file, decision)?;
    Err(MailError::ApprovalDenied)
}

/// Below 3 MiB the file goes as a simple attachment; from there, a session.
async fn upload(
    client: &GraphClient<'_>,
    draft_id: &str,
    name: &str,
    file: &CheckedFile,
) -> Result<AttachmentMeta, MailError> {
    if file.size >= SMALL_ATTACHMENT_LIMIT as u64 {
        return client.upload_session(draft_id, name, file).await;
    }
    let bytes = std::fs::read(&file.path).map_err(unreadable)?;
    let attachment = SmallAttachment {
        file_name: name,
        content_type: content_type(name),
        bytes: &bytes,
    };
    client.add_small_attachment(draft_id, &attachment).await
}

fn attached_file(meta: AttachmentMeta, sha256: String) -> AttachedFile {
    AttachedFile {
        attachment_id: meta.id,
        name: meta.name,
        size: meta.size,
        sha256,
    }
}

/// Graph wants a real media type; the common office and text types are named,
/// everything else is an opaque octet stream.
fn content_type(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}
