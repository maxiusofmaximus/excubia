//! The schema of Alfred's document tool (DOC5B). Alfred only; the host enforces who may call it.

use serde_json::{json, Value};

/// The document tools, in the order the surface lists them.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "doc.create",
            "description": "Crea un documento nuevo (Excel, Word, PDF, CSV, texto) y lo deja en el chat como adjunto. No modifica archivos existentes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["xlsx", "docx", "pdf", "csv", "txt", "md", "json"], "description": "The format of the new file." },
                    "name": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The file name without a path; the host adds the extension." },
                    "content": { "type": "string", "minLength": 1, "maxLength": 10485760, "description": "xlsx: CSV text, the first row is the header (at most 50000 cells). docx and pdf: Markdown. csv, txt, md and json: the text as it is kept; json must be valid." }
                },
                "required": ["format", "name", "content"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "doc.read",
            "description": "Lee el contenido de un documento que el usuario adjuntó en el chat (Excel, Word, PDF, CSV, texto). Devuelve el texto; no modifica nada.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "attachment_id": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The id of the file the user attached (att_…), as the message lists it." },
                    "sheet": { "type": "string", "minLength": 1, "maxLength": 200, "description": "Spreadsheets only: the sheet's name. Default: the first sheet." },
                    "range": { "type": "string", "minLength": 1, "maxLength": 200, "description": "Spreadsheets only: the cells to read, such as A1:H40. Default: the first 200 rows." },
                    "page": { "type": "integer", "minimum": 1, "description": "PDF and Word only: the page to read, from 1. Default: the whole document." }
                },
                "required": ["attachment_id"],
                "additionalProperties": false
            }
        }),
    ]
}
