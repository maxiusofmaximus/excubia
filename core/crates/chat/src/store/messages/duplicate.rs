use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ChatError;
use crate::model::Message;

pub(super) struct Duplicate {
    pub(super) id: String,
    pub(super) seq: i64,
    pub(super) state: String,
    pub(super) body_md: String,
    pub(super) created_at: String,
}

impl Duplicate {
    pub(super) fn into_message(self, conversation_id: &str) -> Message {
        Message {
            id: self.id,
            conversation_id: conversation_id.to_string(),
            seq: self.seq,
            author_kind: "user".to_string(),
            author_agent_id: None,
            body_md: self.body_md,
            state: self.state,
            created_at: self.created_at,
        }
    }
}

pub(super) fn duplicate(
    connection: &Connection,
    conversation_id: &str,
    client_id: &str,
) -> Result<Option<Duplicate>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, seq, state, body_md, created_at FROM message
              WHERE conversation_id = ?1 AND client_id = ?2",
            params![conversation_id, client_id],
            |row| {
                Ok(Duplicate {
                    id: row.get(0)?,
                    seq: row.get(1)?,
                    state: row.get(2)?,
                    body_md: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .optional()?)
}
