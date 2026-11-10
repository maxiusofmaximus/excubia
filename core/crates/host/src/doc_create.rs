//! `doc.create` (DOC8): Alfred makes a document. The host builds it through the document
//! CLIs (`craft`), keeps it as a ready attachment of the conversation and of the run that
//! made it, and audits the creation by tool and attachment id. Failures carry a stable
//! code and a fixed Spanish message: never a path or raw CLI output.

use std::sync::Arc;

use excubia_core_chat::{AttachmentError, SeatActor, ToolError, ToolErrorKind, ToolReply};
use excubia_core_crafting::{safe_name, CraftError, Kind};
use excubia_core_mail::CreateRecord;
use excubia_core_seat::{DocCreate, DocFormat};
use serde_json::json;

use crate::doc_tools::DocTools;

impl DocTools {
    /// Runs one `doc.create` for `actor`.
    pub async fn create(&self, actor: &SeatActor, call: DocCreate) -> Result<ToolReply, ToolError> {
        let kind = kind_of(call.format);
        let name = safe_name(&call.name, kind.extension());
        let craft = Arc::clone(&self.craft);
        let content = call.content;
        let bytes = tokio::task::spawn_blocking(move || craft.create(kind, &content))
            .await
            .map_err(|_| tool_failed())?
            .map_err(create_error)?;
        let attachment = self
            .chat
            .create_agent_file(
                &actor.conversation_id,
                &actor.run_id,
                &name,
                kind.mime(),
                &bytes,
            )
            .map_err(store_error)?;
        self.audit
            .append(&CreateRecord::new("doc.create", &attachment.id))
            .map_err(|_| tool_failed())?;
        Ok(ToolReply {
            result: json!({
                "ok": true,
                "attachment_id": attachment.id,
                "name": name,
                "kind": "document",
                "mime": kind.mime(),
            }),
        })
    }
}

fn kind_of(format: DocFormat) -> Kind {
    match format {
        DocFormat::Xlsx => Kind::Xlsx,
        DocFormat::Docx => Kind::Docx,
        DocFormat::Pdf => Kind::Pdf,
        DocFormat::Csv => Kind::Csv,
        DocFormat::Txt => Kind::Txt,
        DocFormat::Md => Kind::Md,
        DocFormat::Json => Kind::Json,
    }
}

/// What a creation refused or failed at. Only a missing app and a refused content
/// reach the agent with their own words; everything else is one fixed line.
fn create_error(error: CraftError) -> ToolError {
    let (kind, code, field, message) = match &error {
        CraftError::ToolMissing(_) => (
            ToolErrorKind::Unavailable,
            "tool_missing",
            None,
            error.to_string(),
        ),
        CraftError::Invalid(reason) => (
            ToolErrorKind::Invalid,
            "bad_content",
            Some("content"),
            format!("El contenido no sirve: {reason}"),
        ),
        _ => return tool_failed(),
    };
    ToolError {
        kind,
        code,
        field,
        message,
    }
}

fn store_error(error: AttachmentError) -> ToolError {
    match error {
        AttachmentError::ConversationQuota | AttachmentError::HostQuota => ToolError {
            kind: ToolErrorKind::Invalid,
            code: "quota_exceeded",
            field: None,
            message: "No hay espacio para otro archivo en esta conversación".to_string(),
        },
        _ => tool_failed(),
    }
}

fn tool_failed() -> ToolError {
    ToolError {
        kind: ToolErrorKind::Unavailable,
        code: "tool_failed",
        field: None,
        message: "No se pudo crear el documento".to_string(),
    }
}
