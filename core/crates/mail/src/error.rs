//! Every failure of the mail path, with a stable code and a redacted message.
//!
//! A message is never assembled from a raw token, device code or upload URL:
//! the [`Redacted`] fields scrub their text when they are built and again when
//! they are printed, so `Display` and `Debug` are both safe to log.

use crate::redact::Redacted;

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error(
        "mail is not configured: set [mail] client_id in the configuration or \
         EXCUBIA_MAIL_CLIENT_ID in the environment (see docs/contracts/mail.md)"
    )]
    NotConfigured,
    #[error("invalid mail configuration: {0}")]
    InvalidConfig(Redacted),
    #[error("invalid mail input: {0}")]
    InvalidInput(Redacted),
    #[error("mail transport failed: {0}")]
    Transport(Redacted),
    #[error("Microsoft sign-in failed ({code}): {message}")]
    AuthRejected { code: String, message: Redacted },
    #[error("the sign-in code expired; start the device login again")]
    LoginExpired,
    #[error("the sign-in was denied on the Microsoft page")]
    LoginDenied,
    #[error(
        "this tenant requires an administrator to approve the app \
         (AADSTS65001/AADSTS90094); ask a Microsoft 365 administrator to grant consent: {detail}"
    )]
    AdminConsentRequired { detail: Redacted },
    #[error(
        "the configured client_id is not registered for this tenant (AADSTS700016); \
         check the public-client app registration in Microsoft Entra: {detail}"
    )]
    AppNotRegistered { detail: Redacted },
    #[error("the access token was rejected; refresh the account and try again")]
    AuthRequired,
    #[error("Microsoft Graph answered {status} ({code}): {message}")]
    Graph {
        status: u16,
        code: String,
        message: Redacted,
    },
    #[error("Microsoft answered with a body this client cannot read: {0}")]
    BadResponse(Redacted),
    #[error("draft {id} was not created by Excubia in this session")]
    DraftNotOwned { id: Redacted },
    #[error("the message is not in this mailbox")]
    MessageNotFound,
    #[error("the attachment is {size} bytes; files of 3 MiB or more need an upload session")]
    UseUploadSession { size: u64 },
    #[error("the path was rejected: {0}")]
    PathRejected(Redacted),
    /// No file of the conversation has this id, or it is not ready and sent. The same
    /// answer for all of them, so a reply never reveals another conversation.
    #[error("no such attachment in this conversation")]
    AttachmentNotFound,
    #[error("the owner did not approve attaching this file")]
    ApprovalDenied,
    #[error("the file changed after it was approved; nothing was sent")]
    ContentChanged,
    #[error("the approval could not be requested: {0}")]
    ApprovalUnavailable(Redacted),
    #[error("the upload session failed: {0}")]
    UploadFailed(Redacted),
    #[error("the mail account store failed: {0}")]
    Store(Redacted),
    #[error("sending is not enabled for this account; the owner must grant it first")]
    SendNotEnabled,
    #[error("the draft changed after it was approved, or the approval was already used; nothing was sent")]
    ApprovalStale,
    #[error("the owner refused this draft; change it to ask again")]
    DraftRefused,
    #[error("the owner has not answered the send approval yet")]
    ApprovalPending,
    #[error("too many mails were sent from this account in the last hour; wait {retry_after} s")]
    SendRateLimited { retry_after: u64 },
    #[error("no recorded action has that id")]
    ActionNotFound,
    #[error("this action was already undone")]
    ActionUndone,
    #[error(
        "cannot be undone automatically: the message left a folder that undo does not return to"
    )]
    NotUndoable,
    #[error("the moves changed after the owner approved them, or the batch was already asked; nothing was moved")]
    MoveStale,
    #[error("the owner already applied this batch; nothing was moved again")]
    MoveAlreadyApplied,
    #[error("the owner refused this batch of moves; change it to ask again")]
    MoveRefused,
    /// Several approved batches of the account wait to be applied: the caller names one.
    #[error("more than one approved batch waits; name one with approval_id")]
    AmbiguousApproval(Vec<String>),
}

impl MailError {
    /// The stable code the CLI, the HTTP surface and the app branch on. Mail
    /// codes are local to this crate; MAIL1C documents them in `mail.md`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "mail_not_configured",
            Self::InvalidConfig(_) => "mail_invalid_config",
            Self::InvalidInput(_) => "mail_invalid_input",
            Self::Transport(_) => "mail_unavailable",
            Self::AuthRejected { .. } => "mail_auth_failed",
            Self::LoginExpired => "mail_login_expired",
            Self::LoginDenied => "mail_login_denied",
            Self::AdminConsentRequired { .. } => "mail_admin_consent_required",
            Self::AppNotRegistered { .. } => "mail_app_not_registered",
            Self::AuthRequired => "mail_auth_required",
            Self::Graph { .. } => "mail_graph_error",
            Self::BadResponse(_) => "mail_bad_response",
            Self::DraftNotOwned { .. } => "mail_unknown_draft",
            Self::MessageNotFound => "mail_not_found",
            Self::UseUploadSession { .. } => "mail_use_upload_session",
            Self::PathRejected(_) => "mail_path_rejected",
            Self::AttachmentNotFound => "attachment_not_found",
            Self::ApprovalDenied => "mail_approval_denied",
            Self::ContentChanged => "mail_content_changed",
            Self::ApprovalUnavailable(_) => "mail_approval_unavailable",
            Self::UploadFailed(_) => "mail_upload_failed",
            Self::Store(_) => "mail_store_error",
            Self::SendNotEnabled => "mail_send_not_enabled",
            Self::ApprovalStale => "mail_approval_stale",
            Self::DraftRefused => "mail_draft_refused",
            Self::ApprovalPending => "mail_approval_pending",
            Self::SendRateLimited { .. } => "mail_send_rate_limited",
            Self::ActionNotFound => "mail_action_not_found",
            Self::ActionUndone => "mail_action_undone",
            Self::NotUndoable => "mail_not_undoable",
            Self::MoveStale => "mail_move_stale",
            Self::MoveAlreadyApplied => "mail_move_already_applied",
            Self::MoveRefused => "mail_move_refused",
            Self::AmbiguousApproval(_) => "ambiguous_approval",
        }
    }

    /// A refusal the caller can see and change its request for. Everything else is
    /// the host or Graph failing.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self.code(),
            "mail_invalid_input"
                | "mail_path_rejected"
                | "attachment_not_found"
                | "mail_approval_denied"
                | "mail_content_changed"
                | "mail_unknown_draft"
                | "mail_not_found"
                | "mail_use_upload_session"
                | "mail_move_stale"
                | "mail_move_refused"
                | "mail_move_already_applied"
                | "ambiguous_approval"
        )
    }
}
