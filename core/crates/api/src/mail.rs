//! The mail surface the API serves (MAIL1D §1, MAIL2A). The host owns the logins,
//! the sends and the account store and implements [`MailActions`]; the API only
//! carries the answers. No token and no device code ever crosses this boundary:
//! a login answers with what the user types, and the listing carries no credential.

use async_trait::async_trait;
use hn_core_chat::Approval;
use serde::Serialize;

/// The tenants a login may name (MAIL-SPEC §3). Personal accounts are `consumers`.
pub const TENANTS: [&str; 3] = ["consumers", "organizations", "common"];

/// What the user needs to finish a login: the code to type and the page to type
/// it on. The device code stays with the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MailLoginStart {
    pub login_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Where a login stands. `Ok` names the account the login saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MailLoginStatus {
    Pending,
    Ok { label: String, upn: String },
    Denied,
    Expired,
    Error { code: String },
}

/// One saved account: its label, its tenant, its sign-in name and whether the
/// owner granted sending (MAIL2A).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MailAccount {
    pub label: String,
    pub tenant: String,
    pub upn: String,
    pub send_enabled: bool,
}

/// A refusal the host reports with a stable code (`errors.md`) and a fixed,
/// already-redacted message. `retry_after` is set only for a rate limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailFailure {
    pub code: &'static str,
    pub message: &'static str,
    pub retry_after: Option<u64>,
}

/// Where a send request stands. `Awaiting` means the owner has not answered the
/// approval yet, or has just been asked; `Scheduled` means the send is in its
/// cancel window and will leave when the window ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendAnswer {
    Awaiting {
        approval_id: String,
        expires_in: u64,
    },
    Scheduled {
        send_id: String,
        window_seconds: u64,
    },
}

/// Why a send failed: a stable code and what a retry may do. Both are fixed words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendFailure {
    pub code: &'static str,
    pub retry: &'static str,
}

/// A send's public state, and why it failed when it did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendStatus {
    pub state: &'static str,
    pub failure: Option<SendFailure>,
}

/// What the mail routes can do. Implemented by the host.
#[async_trait]
pub trait MailActions: Send + Sync + 'static {
    /// Start a device login for `account` under `tenant` (one of [`TENANTS`]).
    /// With a `conversation_id`, the sign-in card shows there until the login
    /// ends.
    async fn login_start(
        &self,
        account: &str,
        tenant: &str,
        conversation_id: Option<&str>,
    ) -> Result<MailLoginStart, MailFailure>;

    /// The status of the latest login started for `account`.
    fn login_status(&self, account: &str) -> Result<MailLoginStatus, MailFailure>;

    /// The accounts saved on this host.
    fn accounts(&self) -> Result<Vec<MailAccount>, MailFailure>;

    /// Asks the owner to approve sending `draft_id` from `account`. Without an
    /// `approval_id` it opens the approval; with one, it checks that approval and
    /// schedules the send once the owner has said yes.
    async fn send(
        &self,
        account: &str,
        draft_id: &str,
        approval_id: Option<&str>,
    ) -> Result<SendAnswer, MailFailure>;

    /// The public state of a send: `pending`, `sent`, `cancelled` or `failed`, with
    /// the failure when it is `failed`.
    fn send_status(&self, send_id: &str) -> Result<SendStatus, MailFailure>;

    /// Cancels a send still in its window and returns the state after the attempt.
    fn send_cancel(&self, send_id: &str) -> Result<&'static str, MailFailure>;

    /// Starts the second device login that grants sending on `account`.
    async fn send_enable_start(&self, account: &str) -> Result<MailLoginStart, MailFailure>;

    /// Where the sending consent for `account` stands.
    fn send_enable_status(&self, account: &str) -> Result<MailLoginStatus, MailFailure>;

    /// Revokes the sending consent of `account`: its send tokens are deleted.
    fn send_disable(&self, account: &str) -> Result<(), MailFailure>;

    /// Told once per new decision of an approval, after it is saved. It must not wait:
    /// the decision answers first. Nothing by default.
    fn approval_decided(&self, _approval: &Approval) {}

    /// The draft as the operator reads it before approving a send. Reads only.
    async fn draft_review(&self, account: &str, draft_id: &str) -> Result<MailDraft, MailFailure>;

    /// The owner edits a draft by hand. The fields the edit leaves out keep their
    /// value. The answer is the draft as the review serves it; `device_id` is
    /// audited, the content never is.
    async fn draft_update(
        &self,
        account: &str,
        draft_id: &str,
        edit: DraftEdit,
        device_id: &str,
    ) -> Result<MailDraft, MailFailure>;
}

/// What the owner changed in a draft. `None` keeps the field as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftEdit {
    pub to: Option<Vec<String>>,
    pub cc: Option<Vec<String>>,
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// A draft as the operator reviews it before a send: the covered fields, the
/// body in full and the attachments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailDraft {
    pub account: String,
    pub draft_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// `text` or `html`, as Microsoft Graph reports the body.
    pub content_type: String,
    pub attachments: Vec<MailDraftAttachment>,
}

/// One attachment of a reviewed draft. `sha256` is absent for a file that was
/// added outside Excubia, so its bytes were never seen here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailDraftAttachment {
    pub name: String,
    pub size: u64,
    pub sha256: Option<String>,
}
