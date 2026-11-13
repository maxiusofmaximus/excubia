//! `doc.read` (DOC5B): Alfred reads a document the user sent in this conversation.
//! The file is found the way the mail tools find one (`chat_file`), read through the
//! host's CLIs (`craft`), and the read is audited by tool and attachment id only.
//! Failures carry a stable code and a fixed message: never a path or raw CLI output.

use std::sync::Arc;

use excubia_chat::{ChatStore, SeatActor, ToolError, ToolErrorKind, ToolReply};
use excubia_crafting::{CraftError, Document, Kind, ReadOpts};
use excubia_mail::{AuditLog, MailError, ReadRecord};
use excubia_seat::DocRead;
use serde_json::json;

use crate::chat_file;
use crate::craft::Craft;

/// The reader of `doc.read`: the chat files, the CLIs and the audit line.
pub struct DocTools {
    pub(crate) craft: Arc<Craft>,
    pub(crate) chat: Arc<ChatStore>,
    pub(crate) audit: AuditLog,
}

impl DocTools {
    pub fn new(craft: Arc<Craft>, chat: Arc<ChatStore>, audit: AuditLog) -> Self {
        Self { craft, chat, audit }
    }

    /// Runs one `doc.read` for `actor`.
    pub async fn call(&self, actor: &SeatActor, call: DocRead) -> Result<ToolReply, ToolError> {
        let file = chat_file::resolve(&self.chat, &actor.conversation_id, &call.attachment_id)
            .map_err(file_error)?;
        let kind = Kind::of(&file.name, &file.mime).map_err(read_error)?;
        let name = file.name.clone();
        let doc = Document {
            path: file.path,
            name: file.name,
            mime: file.mime,
        };
        let opts = ReadOpts {
            sheet: call.sheet,
            range: call.range,
            page: call.page,
        };
        let craft = Arc::clone(&self.craft);
        let out = tokio::task::spawn_blocking(move || craft.read(&doc, &opts))
            .await
            .map_err(|_| unavailable("No se pudo leer el documento"))?
            .map_err(read_error)?;
        self.audit
            .append(&ReadRecord::new("", "doc.read", Some(&call.attachment_id)))
            .map_err(|_| unavailable("No se pudo guardar la lectura en el registro"))?;
        Ok(ToolReply {
            result: json!({
                "ok": true,
                "untrusted_content": true,
                "name": name,
                "kind": kind_label(kind),
                "text": out.text,
                "truncated": out.truncated,
                "parts": out.pages_or_sheets,
            }),
        })
    }
}

fn file_error(error: MailError) -> ToolError {
    match error {
        MailError::AttachmentNotFound => ToolError {
            kind: ToolErrorKind::Invalid,
            code: "attachment_not_found",
            field: Some("attachment_id"),
            message: "Ese documento no está en esta conversación o todavía no se ha enviado"
                .to_string(),
        },
        _ => unavailable("No se pudo leer el documento"),
    }
}

/// What a document CLI refused or failed at, in the terms the agent can act on.
fn read_error(error: CraftError) -> ToolError {
    let text = error.to_string();
    let (kind, code, field, message) = match error {
        CraftError::ToolMissing(_) => (ToolErrorKind::Unavailable, "tool_missing", None, text),
        CraftError::Unsupported(_) => (
            ToolErrorKind::Invalid,
            "unsupported_document",
            Some("attachment_id"),
            "Este tipo de documento todavía no se puede leer".to_string(),
        ),
        CraftError::BadDocument(_) => (
            ToolErrorKind::Invalid,
            "bad_document",
            Some("attachment_id"),
            "El documento no se puede leer: no es un archivo válido".to_string(),
        ),
        CraftError::PageOutOfRange(_) => (
            ToolErrorKind::Invalid,
            "page_out_of_range",
            Some("page"),
            text,
        ),
        CraftError::Invalid(_) => (
            ToolErrorKind::Invalid,
            "invalid_arguments",
            Some("page"),
            text,
        ),
        _ => (
            ToolErrorKind::Unavailable,
            "tool_failed",
            None,
            "No se pudo leer el documento".to_string(),
        ),
    };
    ToolError {
        kind,
        code,
        field,
        message,
    }
}

fn unavailable(message: &str) -> ToolError {
    ToolError {
        kind: ToolErrorKind::Unavailable,
        code: "tool_failed",
        field: None,
        message: message.to_string(),
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Xlsx => "xlsx",
        Kind::Docx => "docx",
        Kind::Pdf => "pdf",
        Kind::Csv => "csv",
        Kind::Txt => "txt",
        Kind::Md => "md",
        Kind::Json => "json",
    }
}
