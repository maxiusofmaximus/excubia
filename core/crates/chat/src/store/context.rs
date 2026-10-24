//! The bounded conversation window sent to a CLI. Newest messages win: at most
//! 20 messages and 24 000 characters, never the whole history
//! (`orchestrator-chat.md` §8).

use rusqlite::params;
use rusqlite::OptionalExtension;

use crate::error::ChatError;
use crate::model::{ContextPage, Message};

impl super::ChatStore {
    pub fn context(
        &self,
        conversation_id: &str,
        max_messages: usize,
        max_chars: usize,
    ) -> Result<ContextPage, ChatError> {
        let connection = self.lock();
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM conversation WHERE id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(ChatError::NoSuchConversation);
        }
        let mut statement = connection.prepare(
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at
               FROM message WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2",
        )?;
        // Fetch one extra row so "there are older messages" is observable and
        // the prompt can tell the agent that the window was cut.
        let fetch = max_messages.saturating_add(1).min(i64::MAX as usize) as i64;
        let mut rows = statement
            .query_map(params![conversation_id, fetch], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    conversation_id: conversation_id.to_string(),
                    seq: row.get(1)?,
                    author_kind: row.get(2)?,
                    author_agent_id: row.get(3)?,
                    body_md: row.get(4)?,
                    state: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut truncated = rows.len() > max_messages;
        if truncated {
            // Rows are newest first: the last one is the oldest fetched row.
            rows.pop();
        }
        let mut kept = Vec::new();
        let mut used = 0;
        for mut message in rows.drain(..) {
            let count = message.body_md.chars().count();
            if used + count <= max_chars {
                used += count;
                kept.push(message);
                continue;
            }
            if kept.is_empty() && max_chars > 0 {
                // The newest message alone is over budget: keep its head so
                // the agent still sees what the user just asked.
                let keep = max_chars.saturating_sub(1);
                message.body_md = format!(
                    "{}…",
                    message.body_md.chars().take(keep).collect::<String>()
                );
                kept.push(message);
            }
            truncated = true;
            break;
        }
        kept.reverse();
        Ok(ContextPage {
            messages: kept,
            truncated,
        })
    }
}
