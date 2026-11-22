//! `doc.edit` (FIX2): Alfred makes an edited copy of a `.docx` the user sent in the chat.
//! The original is never changed: the copy is a new ready attachment of the conversation and
//! of the run that made it, and the edit is audited by tool and attachment id. Failures carry
//! a stable code and a fixed Spanish message: never a path.

use excubia_chat::{SeatActor, ToolError, ToolErrorKind, ToolReply};
use excubia_crafting::{edit_docx, safe_name, Change, CraftError, Kind};
use excubia_mail::CreateRecord;
use excubia_seat::{DocChange, DocEdit};
use serde_json::json;

use crate::chat_file;
use crate::doc_create::store_error;
use crate::doc_tools::{file_error, DocTools};

impl DocTools {
    /// Runs one `doc.edit` for `actor`.
    pub async fn edit(&self, actor: &SeatActor, call: DocEdit) -> Result<ToolReply, ToolError> {
        let file = chat_file::resolve(&self.chat, &actor.conversation_id, &call.attachment_id)
            .map_err(file_error)?;
        if !matches!(Kind::of(&file.name, &file.mime), Ok(Kind::Docx)) {
            return Err(ToolError {
                kind: ToolErrorKind::Invalid,
                code: "unsupported_document",
                field: Some("attachment_id"),
                message: "Solo se pueden editar documentos Word (.docx)".to_string(),
            });
        }
        let name = edited_name(&file.name);
        let path = file.path;
        let changed = call.changes.len();
        let changes = call.changes;
        let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, CraftError> {
            let original = std::fs::read(&path)?;
            let edits: Vec<Change<'_>> = changes.iter().map(as_change).collect();
            edit_docx(&original, &edits)
        })
        .await
        .map_err(|_| edit_failed())?
        .map_err(edit_error)?;
        let attachment = self
            .chat
            .create_agent_file(
                &actor.conversation_id,
                &actor.run_id,
                &name,
                Kind::Docx.mime(),
                &bytes,
            )
            .map_err(store_error)?;
        self.audit
            .append(&CreateRecord::new("doc.edit", &attachment.id))
            .map_err(|_| edit_failed())?;
        Ok(ToolReply {
            result: json!({
                "ok": true,
                "attachment_id": attachment.id,
                "name": name,
                "kind": "document",
                "mime": Kind::Docx.mime(),
                "changes": changed,
                "original_unchanged": true,
            }),
        })
    }
}

fn as_change(change: &DocChange) -> Change<'_> {
    match change {
        DocChange::ReplaceText { find, replace } => Change::Replace {
            find: find.as_str(),
            replace: replace.as_str(),
        },
        DocChange::SetCell {
            table,
            row,
            col,
            text,
        } => Change::Cell {
            table: *table,
            row: *row,
            col: *col,
            text: text.as_str(),
        },
    }
}

/// `Informe.docx` becomes `Informe (editado).docx`, made safe like any created file.
fn edited_name(original: &str) -> String {
    let stem = std::path::Path::new(original)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("documento");
    safe_name(&format!("{stem} (editado)"), Kind::Docx.extension())
}

/// What an edit refused or failed at. The text of a refused change is the user's own
/// words; everything else is one fixed line.
fn edit_error(error: CraftError) -> ToolError {
    match error {
        CraftError::Target(message) => ToolError {
            kind: ToolErrorKind::Invalid,
            code: "target_not_found",
            field: Some("changes"),
            message,
        },
        CraftError::Invalid(reason) => ToolError {
            kind: ToolErrorKind::Invalid,
            code: "invalid_arguments",
            field: Some("changes"),
            message: format!("El cambio no sirve: {reason}"),
        },
        CraftError::BadDocument(_) => ToolError {
            kind: ToolErrorKind::Invalid,
            code: "bad_document",
            field: Some("attachment_id"),
            message: "El documento no se puede editar: no es un archivo válido".to_string(),
        },
        _ => edit_failed(),
    }
}

fn edit_failed() -> ToolError {
    ToolError {
        kind: ToolErrorKind::Unavailable,
        code: "tool_failed",
        field: None,
        message: "No se pudo editar el documento".to_string(),
    }
}
