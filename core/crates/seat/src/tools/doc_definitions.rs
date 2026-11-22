//! The schema of Alfred's document tool (DOC5B). Alfred only; the host enforces who may call it.

use serde_json::{json, Value};

/// The document tools, in the order the surface lists them.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "doc.create",
            "description": "Crea un documento nuevo (Excel, Word, PDF, CSV, texto) y lo deja en el chat como adjunto. No modifica archivos existentes. Antes de redactar, llama a `style.get` y sigue sus reglas.",
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
            "name": "doc.edit",
            "description": "Hace una copia editada de un documento Word (.docx) que el usuario adjuntó en el chat, con los cambios indicados; el original no se modifica. La copia queda en el chat como adjunto nuevo: ponla en attachment_ids de chat.reply. Solo cambia el texto y las celdas; el formato se conserva. Antes de editar, lee el documento con doc.read.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "attachment_id": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The id of the .docx the user attached (att_…)." },
                    "changes": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 200,
                        "description": "The changes, applied in order. replace_text changes every match in the document, also inside tables. set_cell writes one cell.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": { "type": "string", "enum": ["replace_text", "set_cell"] },
                                "find": { "type": "string", "minLength": 1, "maxLength": 32768, "description": "replace_text: the text to find, as it reads in Word." },
                                "replace": { "type": "string", "maxLength": 32768, "description": "replace_text: the new text." },
                                "table": { "type": "integer", "minimum": 1, "description": "set_cell: the table, from 1, in the order it appears in the document." },
                                "row": { "type": "integer", "minimum": 1, "description": "set_cell: the row of the table, from 1." },
                                "col": { "type": "integer", "minimum": 1, "description": "set_cell: the column of the row, from 1." },
                                "text": { "type": "string", "maxLength": 32768, "description": "set_cell: the new text of the cell." }
                            },
                            "required": ["op"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["attachment_id", "changes"],
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
