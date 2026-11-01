//! The draft the product builds and the validation it must pass before a
//! single byte reaches Graph.
//!
//! Validation is deliberately local and strict: bare addresses only (no
//! display names, no control characters), a plain attachment file name and an
//! explicit text/html body. A malformed draft is refused with a typed error,
//! never sent and retried.

use std::borrow::Cow;

use serde_json::{json, Value};

use crate::error::MailError;
use crate::markup::{html_to_text, HTML_CONTENT_TYPE, TEXT_CONTENT_TYPE};
use crate::redact::Redacted;

const MAX_SUBJECT_CHARS: usize = 998;
const MAX_NAME_CHARS: usize = 255;

/// Graph stores an all-ASCII text body as us-ascii and mangles the non-ASCII sender name, so one hard space forces UTF-8.
pub(crate) const ASCII_TEXT_PADDING: char = '\u{a0}';

/// The body of a draft, in one of the two formats the product offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftBody {
    Text(String),
    Html(String),
}

impl DraftBody {
    /// The `contentType` Graph stores: the body goes as the owner wrote it.
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::Text(_) => TEXT_CONTENT_TYPE,
            Self::Html(_) => HTML_CONTENT_TYPE,
        }
    }

    /// The body as the owner wrote it.
    pub fn content(&self) -> &str {
        match self {
            Self::Text(text) | Self::Html(text) => text,
        }
    }

    /// The content Graph must receive: an all-ASCII text body gains one hard
    /// space, which is what keeps Graph from labelling the message us-ascii.
    pub(crate) fn graph_content(&self) -> Cow<'_, str> {
        match self {
            Self::Text(text) if text.is_ascii() => {
                Cow::Owned(format!("{text}{ASCII_TEXT_PADDING}"))
            }
            Self::Text(text) | Self::Html(text) => Cow::Borrowed(text),
        }
    }

    /// The body as the operator reads it.
    pub fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Html(html) => html_to_text(html),
        }
    }
}

/// What `mail.draft_create` and `mail.draft_update` accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInput {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: DraftBody,
}

/// The minimal answer the tools return: no body, no recipients beyond the
/// draft's own, and the names of its attachments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftMeta {
    pub id: String,
    pub subject: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub is_draft: bool,
    pub attachment_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmallAttachment<'a> {
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    pub size: u64,
}

impl DraftInput {
    /// The Graph message payload. Its key order is stable, which is what makes
    /// the idempotency hash of `create_draft` meaningful.
    pub(crate) fn payload(&self) -> Result<Value, MailError> {
        validate_subject(&self.subject)?;
        validate_recipients(&self.to, "to")?;
        validate_recipients(&self.cc, "cc")?;
        Ok(json!({
            "subject": self.subject,
            "body": {
                "contentType": self.body.content_type(),
                "content": self.body.graph_content(),
            },
            "toRecipients": recipients(&self.to),
            "ccRecipients": recipients(&self.cc),
        }))
    }
}

fn validate_subject(subject: &str) -> Result<(), MailError> {
    if subject.chars().count() > MAX_SUBJECT_CHARS || subject.chars().any(char::is_control) {
        return Err(MailError::InvalidInput(Redacted::new(
            "a subject is at most 998 characters and has no control characters",
        )));
    }
    Ok(())
}

fn validate_recipients(addresses: &[String], field: &str) -> Result<(), MailError> {
    for address in addresses {
        let bare = !address.trim().is_empty()
            && address.trim() == address
            && address.contains('@')
            && !address
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control());
        if !bare {
            return Err(MailError::InvalidInput(Redacted::new(format!(
                "{field} contains \"{address}\", which is not a bare email address"
            ))));
        }
    }
    Ok(())
}

fn recipients(addresses: &[String]) -> Vec<Value> {
    addresses
        .iter()
        .map(|address| json!({"emailAddress": {"address": address}}))
        .collect()
}

pub(crate) fn validate_attachment(attachment: &SmallAttachment<'_>) -> Result<(), MailError> {
    let name = attachment.file_name;
    let valid_name = !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && !name.chars().any(char::is_control)
        && !name.contains('/')
        && !name.contains('\\');
    let valid_type = attachment.content_type.contains('/')
        && !attachment.content_type.chars().any(char::is_control);
    if !valid_name || !valid_type {
        return Err(MailError::InvalidInput(Redacted::new(
            "an attachment needs a plain file name and a content type such as application/pdf",
        )));
    }
    Ok(())
}
