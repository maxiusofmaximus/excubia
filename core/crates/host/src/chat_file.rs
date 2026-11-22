//! A file the user sent in the chat, as the host's tools may read or attach it (DOC2,
//! DOC5B). It is found only in the conversation it was sent in, and only once it is
//! ready and part of a message. Every other id gets the same answer, so a reply never
//! reveals whether the file exists somewhere else.

use std::path::PathBuf;

use excubia_chat::{Attachment, AttachmentState, ChatStore};
use excubia_mail::{MailError, Redacted};

/// The stored file of a chat attachment, and the name and mime type the user gave it.
pub(crate) struct ChatFile {
    pub path: PathBuf,
    pub name: String,
    pub mime: String,
}

pub(crate) fn resolve(
    chat: &ChatStore,
    conversation_id: &str,
    attachment_id: &str,
) -> Result<ChatFile, MailError> {
    let attachment = chat
        .attachment(attachment_id)
        .ok()
        .filter(|attachment| usable(attachment, conversation_id))
        .ok_or(MailError::AttachmentNotFound)?;
    let files = chat
        .attachment_files()
        .ok_or_else(|| MailError::Store(Redacted::new("the attachments folder is not set")))?;
    let path = files
        .file_path(conversation_id, attachment_id)
        .map_err(|_| MailError::AttachmentNotFound)?;
    Ok(ChatFile {
        path,
        name: attachment.name,
        mime: attachment.mime.unwrap_or_default(),
    })
}

fn usable(attachment: &Attachment, conversation_id: &str) -> bool {
    attachment.conversation_id == conversation_id
        && attachment.state == AttachmentState::Ready
        && attachment.message_id.is_some()
}
