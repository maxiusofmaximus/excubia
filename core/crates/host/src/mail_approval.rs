//! Asking the user before a file outside the workspace is attached (MAIL1C §1).
//!
//! The question is a chat approval of kind `tool` under the action
//! `mail.attach_path`, bound to the digest the path policy computed: the same
//! bytes are the same question, and a changed file is a new one. Only a device
//! with APPROVE answers it; nothing in this module decides for the user.

use async_trait::async_trait;
use excubia_core_chat::{ApprovalKind, ApprovalStatus, ChatStore, NewApproval};
use excubia_core_mail::{Approval, ApprovalGate, MailError, PathApproval, Redacted};
use serde_json::json;

/// The action the approval is filed under. Also the `tool` of the question.
pub const ACTION: &str = "mail.attach_path";

/// How long the device has to answer before the question expires.
pub const DEFAULT_TTL_SECONDS: u32 = 120;

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
    ttl_seconds: u32,
}

impl<'a> ChatApprovalGate<'a> {
    /// `workspace` is the folder the attachment is judged against.
    pub fn new(chat: &'a ChatStore, workspace: impl Into<String>) -> Self {
        Self {
            chat,
            workspace: workspace.into(),
            conversation_id: None,
            ttl_seconds: DEFAULT_TTL_SECONDS,
        }
    }

    pub fn in_conversation(mut self, conversation_id: impl Into<String>) -> Self {
        self.conversation_id = Some(conversation_id.into());
        self
    }

    pub fn with_ttl(mut self, ttl_seconds: u32) -> Self {
        self.ttl_seconds = ttl_seconds;
        self
    }

    fn question(&self, request: &PathApproval) -> NewApproval {
        let file = &request.file;
        let path = file.path.to_string_lossy().into_owned();
        NewApproval {
            conversation_id: self.conversation_id.clone(),
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: ACTION.to_owned(),
            summary: format!(
                "Adjuntar {path}: {} bytes, SHA-256 {}",
                file.size, file.sha256
            ),
            cwd: Some(self.workspace.clone()),
            paths: vec![path.clone()],
            risk: None,
            action_digest: format!("sha256:{}", request.digest),
            payload_redacted: Some(json!({
                "path": path,
                "size": file.size,
                "sha256": file.sha256,
            })),
            ttl_seconds: self.ttl_seconds,
        }
    }
}

#[async_trait]
impl ApprovalGate for ChatApprovalGate<'_> {
    async fn decide(&self, request: &PathApproval) -> Result<Approval, MailError> {
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
