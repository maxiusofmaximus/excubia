//! Attaching a file outside the workspace once the operator answers its card. The draft
//! was made without waiting; the decision runs this in the background, and the
//! conversation hears what happened. The file must still be the bytes the card showed.

use std::path::Path;

use async_trait::async_trait;
use excubia_chat::{Approval, ApprovalStatus, ChatError};
use excubia_mail::attach::{attach_file, AttachContext, AttachedFile};
use excubia_mail::{ApprovalGate, MailError, PathApproval};

use crate::mail_approval::ACTION;
use crate::mail_move::chat_error;
use crate::mail_tools::MailTools;

impl MailTools {
    /// Attaches the file of an approved card to its draft, or tells the conversation it was
    /// denied. Another tool's card, or one without its draft, does nothing.
    pub async fn attach_decided(&self, approval: &Approval) {
        if approval.tool != ACTION {
            return;
        }
        let Some(payload) = approval.payload_redacted.as_ref() else {
            return;
        };
        let (Some(account), Some(draft_id), Some(path)) = (
            payload["mail_account"].as_str(),
            payload["draft_id"].as_str(),
            payload["path"].as_str(),
        ) else {
            return;
        };
        let report = match approval.status {
            ApprovalStatus::Approved => {
                match self
                    .attach_approved(approval, account, draft_id, path)
                    .await
                {
                    Ok(Some(attached)) => format!(
                        "Adjunté {} al borrador. Ya puedes revisarlo.",
                        attached.name
                    ),
                    // Another run burned the card first and reports it.
                    Ok(None) => return,
                    Err(error) => format!("No pude adjuntar {path}: {}.", error.code()),
                }
            }
            ApprovalStatus::Denied => {
                format!("No adjunté {path}: lo denegaste. El borrador sigue sin ese archivo.")
            }
            _ => return,
        };
        let Some(conversation) = approval.conversation_id.as_deref() else {
            return;
        };
        if let Err(error) = self.chat.append_alfred_message(conversation, &report) {
            eprintln!("host: could not write the report of an attachment: {error}");
        }
    }

    /// Burns the card, then attaches through the same checks as any attachment. `None`
    /// when the card was already spent.
    async fn attach_approved(
        &self,
        approval: &Approval,
        account: &str,
        draft_id: &str,
        path: &str,
    ) -> Result<Option<AttachedFile>, MailError> {
        match self
            .chat
            .consume_for_host(&approval.id, &approval.action_digest)
        {
            Ok(_) => {}
            Err(ChatError::ApprovalAlreadyDecided) => return Ok(None),
            Err(error) => return Err(chat_error(error)),
        }
        let access = self.session(account).await?;
        let client = self.client(&access, account);
        let gate = Answered(approval.action_digest.clone());
        let ctx = AttachContext {
            client: &client,
            account,
            policy: &self.policy,
            approval: &gate,
            audit: &self.audit,
            scratch: &self.scratch,
        };
        let attached = attach_file(&ctx, draft_id, Path::new(path)).await?;
        self.sends.draft_changed(account, draft_id)?;
        Ok(Some(attached))
    }
}

/// The operator's yes, valid only for the bytes the card was asked with.
struct Answered(String);

#[async_trait]
impl ApprovalGate for Answered {
    async fn decide(&self, request: &PathApproval) -> Result<excubia_mail::Approval, MailError> {
        if format!("sha256:{}", request.digest) == self.0 {
            Ok(excubia_mail::Approval::Approved)
        } else {
            Err(MailError::ContentChanged)
        }
    }
}
