//! Microsoft 365 mail drafts for the Excubia host.
//!
//! The crate prepares real Outlook drafts without asking the user to install
//! anything: OAuth 2.0 device-code login against Entra ID, tokens encrypted at
//! rest, and a Graph client whose vocabulary is *create a draft*, *update
//! it*, *read its metadata* and *attach a file*. There is no delete and no
//! write to received mail; the inbox can be listed and read, with GET only
//! (MAIL-LECTURA, [`inbox`] and [`received`]). The permission lists are closed in
//! code: [`SCOPES`] for login, and [`SEND_SCOPES`] for a separate consent that
//! lets a human send an approved draft (MAIL2A). Login never asks for
//! `Mail.Send`, and the send path is not an agent tool.
//!
//! Files reach a draft by local path ([`attach_file`]): [`PathPolicy`] decides
//! which paths are allowed or need approval, files of 3 MiB or more go through
//! an upload session, and every decision is written to the [`AuditLog`].
//!
//! Contract: `docs/contracts/mail.md` (MAIL1C, extended by MAIL2A).

#![forbid(unsafe_code)]

pub mod actions;
pub mod attach;
pub mod audit;
pub mod auth;
pub mod config;
mod draft;
mod error;
pub mod folders;
pub mod graph;
mod graph_send;
pub mod inbox;
mod ledger;
mod ledger_question;
mod ledger_send;
mod markup;
mod master_key;
pub mod mover;
pub mod paths;
mod private;
pub mod received;
pub mod redact;
pub mod sanitize;
pub mod send;
mod send_failure;
mod snapshot;
pub mod store;
mod template;
pub mod transport;
pub mod triage;
pub mod undo;
pub mod upload;

pub use actions::{ActionLog, MailActionRecord};
pub use attach::{
    attach_chat_file, attach_file, Approval, ApprovalGate, AttachContext, AttachedFile,
};
pub use audit::{
    domain_of, AuditLog, AuditRecord, CreateRecord, Decision, EditRecord, ReadRecord, SendRecord,
    SendResult,
};
pub use auth::{
    assert_scopes_safe, assert_send_scopes_safe, poll_device_login, refresh, refresh_send,
    start_device_login, start_send_consent, DeviceLogin, Tenant, Tokens, SCOPES, SEND_SCOPES,
};
pub use config::{
    MailConfig, DEFAULT_MAX_ATTACHMENT_BYTES, DEFAULT_SENDS_PER_HOUR, HARD_MAX_ATTACHMENT_BYTES,
    HARD_MAX_SENDS_PER_HOUR,
};
pub use draft::{AttachmentMeta, DraftBody, DraftInput, DraftMeta, SmallAttachment};
pub use error::MailError;
pub use folders::{Folder, FolderAliases};
pub use graph::{GraphClient, SMALL_ATTACHMENT_LIMIT};
pub use ledger::{Ledger, RETENTION_SECONDS};
pub use ledger_question::{Closing, SendQuestion};
pub use ledger_send::SendRow;
pub use mover::{batch_digest, MoveDest, Moved};
pub use paths::{CheckedFile, PathApproval, PathDecision, PathPolicy};
pub use redact::{redact, Redacted, Secret};
pub use sanitize::{sanitize_html, sanitize_text, Policy, Sanitized};
pub use send::{
    check_budget, DraftReview, SendAttachment, SendState, SendView, RATE_WINDOW_SECONDS,
};
pub use send_failure::{FailCode, Retry, SendFailure};
pub use store::{Account, AccountStore, FileKey, KeySource};
pub use template::{agent_html, agent_text};
pub use transport::{HttpResponse, HttpTransport, ReqwestTransport};
pub use upload::{DEFAULT_UPLOAD_CHUNK_BYTES, MAX_UPLOAD_CHUNK_BYTES, UPLOAD_GRANULE_BYTES};
