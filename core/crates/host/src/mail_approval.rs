//! Asking the user before a file outside the workspace is attached (MAIL1C §1).
//!
//! The question is a chat approval of kind `tool` under the action
//! `mail.attach_path`, bound to the digest the path policy computed: the same
//! bytes are the same question, and a changed file is a new one. Only a device
//! with APPROVE answers it; nothing in this module decides for the user.

use std::sync::Mutex;

use async_trait::async_trait;
use excubia_chat::{
    ApprovalKind, ApprovalStatus, ChatStore, NewApproval, ATTACH_PATH_MAX_TTL_SECONDS,
};
use excubia_mail::{Approval, ApprovalGate, MailError, PathApproval, Redacted};
use serde_json::json;

/// The action the approval is filed under. Also the `tool` of the question.
pub const ACTION: &str = "mail.attach_path";

/// How long the device has to answer before the question expires.
pub const DEFAULT_TTL_SECONDS: u32 = 300;

/// Asks the chat approval inbox. A question that expires is [`Approval::Expired`],
/// not a denial, and nothing is sent either way. A question that cannot be
/// placed is not an answer at all: the caller gets
/// [`MailError::ApprovalUnavailable`] instead. Every path reaches
/// outside the workspace by definition, so the question is never delegated: it
/// is placed as `OutsideWorkspace` whatever seat asks and whether it has a
/// project (MAIL1D §0).
pub struct ChatApprovalGate<'a> {
    chat: &'a ChatStore,
    workspace: String,
    conversation_id: Option<String>,
    /// The account and draft the file joins once approved; the payload carries them.
    draft: Option<(String, String)>,
    ttl_seconds: u32,
    /// The question [`Self::decide`] left open for a draft, if any.
    asked: Mutex<Option<String>>,
}

impl<'a> ChatApprovalGate<'a> {
    /// `workspace` is the folder the attachment is judged against.
    pub fn new(chat: &'a ChatStore, workspace: impl Into<String>) -> Self {
        Self {
            chat,
            workspace: workspace.into(),
            conversation_id: None,
            draft: None,
            ttl_seconds: DEFAULT_TTL_SECONDS,
            asked: Mutex::new(None),
        }
    }

    pub fn in_conversation(mut self, conversation_id: impl Into<String>) -> Self {
        self.conversation_id = Some(conversation_id.into());
        self
    }

    /// The question is about `draft_id` of `account`, and waits for the operator as long as
    /// the store lets it: the draft is not held up, the file joins it when he answers.
    pub fn for_draft(mut self, account: &str, draft_id: &str) -> Self {
        self.draft = Some((account.to_owned(), draft_id.to_owned()));
        self.ttl_seconds = ATTACH_PATH_MAX_TTL_SECONDS;
        self
    }

    /// Files the question and returns at once, without waiting for the answer. A question
    /// already open for the same file is returned as it is.
    pub fn ask(&self, request: &PathApproval) -> Result<excubia_chat::Approval, MailError> {
        self.chat
            .create_outside_approval(self.question(request))
            .map_err(unavailable)
    }

    /// The id of the question left open by the last [`Self::decide`] for a draft.
    pub fn asked(&self) -> Option<String> {
        self.asked.lock().ok().and_then(|asked| asked.clone())
    }

    pub fn with_ttl(mut self, ttl_seconds: u32) -> Self {
        self.ttl_seconds = ttl_seconds;
        self
    }

    fn question(&self, request: &PathApproval) -> NewApproval {
        let file = &request.file;
        let path = file.path.to_string_lossy().into_owned();
        // What the operator reads: the path without Windows' verbatim prefix.
        let shown = path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned();
        let mut payload = json!({
            "path": shown,
            "size": file.size,
            "sha256": file.sha256,
        });
        if let Some((account, draft_id)) = &self.draft {
            // Not `account`: a phone reads a card with an account as a mail send.
            payload["mail_account"] = json!(account);
            payload["draft_id"] = json!(draft_id);
        }
        NewApproval {
            conversation_id: self.conversation_id.clone(),
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: ACTION.to_owned(),
            summary: format!("Adjuntar {shown} ({} bytes)", file.size),
            cwd: Some(self.workspace.clone()),
            paths: vec![path.clone()],
            risk: None,
            action_digest: format!("sha256:{}", request.digest),
            payload_redacted: Some(payload),
            ttl_seconds: self.ttl_seconds,
        }
    }
}

#[async_trait]
impl ApprovalGate for ChatApprovalGate<'_> {
    async fn decide(&self, request: &PathApproval) -> Result<Approval, MailError> {
        if self.draft.is_some() {
            // A question for a draft never holds the turn: the host attaches on the answer.
            let asked = self.ask(request)?;
            return match asked.status {
                ApprovalStatus::Denied => Ok(Approval::Denied),
                ApprovalStatus::Expired => Ok(Approval::Expired),
                ApprovalStatus::Pending | ApprovalStatus::Approved | ApprovalStatus::Consumed => {
                    if let Ok(mut open) = self.asked.lock() {
                        *open = Some(asked.id);
                    }
                    Ok(Approval::Asked)
                }
            };
        }
        let decided = self
            .chat
            .request_outside_approval(self.question(request))
            .await
            .map_err(unavailable)?;
        match decided.status {
            ApprovalStatus::Approved | ApprovalStatus::Consumed => Ok(Approval::Approved),
            ApprovalStatus::Denied => Ok(Approval::Denied),
            ApprovalStatus::Expired => Ok(Approval::Expired),
            ApprovalStatus::Pending => Err(unavailable("the question is still open")),
        }
    }
}

fn unavailable(reason: impl ToString) -> MailError {
    MailError::ApprovalUnavailable(Redacted::new(reason.to_string()))
}
