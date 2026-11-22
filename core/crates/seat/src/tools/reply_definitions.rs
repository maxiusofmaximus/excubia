//! The schema of `chat.reply`, the one tool that completes a turn. Its files (DOC8) are
//! the ones Alfred created in this conversation; the host checks each id.

use serde_json::{json, Value};

use super::definitions::string_property;

pub(super) fn chat_reply() -> Value {
    json!({
        "name": "chat.reply",
        "description": "Send Alfred's final answer to the chat. This is the only way a chat turn can complete successfully.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": string_property("The complete answer, in the user's language."),
                "suggestions": {
                    "type": "array",
                    "maxItems": 4,
                    "items": { "type": "string", "minLength": 1, "maxLength": 60 },
                    "description": "Optional quick replies the user can send with one tap, for a short closed choice only."
                },
                "attachment_ids": {
                    "type": "array",
                    "maxItems": 4,
                    "uniqueItems": true,
                    "items": { "type": "string", "minLength": 1, "maxLength": 64 },
                    "description": "Optional. The ids of the files you created with doc.create in this conversation, shown on this answer. Any other id is refused."
                }
            },
            "required": ["text"],
            "additionalProperties": false
        }
    })
}
