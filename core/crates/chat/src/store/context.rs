//! The bounded conversation window sent to a CLI. Newest messages win: at most
//! 20 messages and 24 000 characters, never the whole history
//! (`orchestrator-chat.md` §8).

use std::collections::HashMap;

use rusqlite::params;
use rusqlite::OptionalExtension;

use super::attachments::linked_files;
use crate::error::ChatError;
use crate::model::{ContextAttachment, ContextPage, Message};
use crate::model_attachment::LinkedFile;

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
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at, reply_to_id
               FROM message WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2",
        )?;
        // Fetch one extra row so "there are older messages" is observable and
        // the prompt can tell the agent that the window was cut.
        let fetch = max_messages.saturating_add(1).min(i64::MAX as usize) as i64;
        let fetched = statement
            .query_map(params![conversation_id, fetch], |row| {
                let message = Message {
                    id: row.get(0)?,
                    conversation_id: conversation_id.to_string(),
                    seq: row.get(1)?,
                    author_kind: row.get(2)?,
                    author_agent_id: row.get(3)?,
                    body_md: row.get(4)?,
                    state: row.get(5)?,
                    created_at: row.get(6)?,
                };
                Ok((message, row.get::<_, Option<String>>(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut cited: HashMap<String, String> = fetched
            .iter()
            .filter_map(|(message, reply)| Some((message.id.clone(), reply.clone()?)))
            .collect();
        let mut rows: Vec<Message> = fetched.into_iter().map(|(message, _)| message).collect();

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
        cited.retain(|message_id, _| kept.iter().any(|message| &message.id == message_id));
        let ids: Vec<&str> = kept.iter().map(|message| message.id.as_str()).collect();
        let linked = linked_files(&connection, &ids)?;
        let attachments = self.context_attachments(conversation_id, linked)?;
        Ok(ContextPage {
            messages: kept,
            replies: cited,
            attachments,
            truncated,
        })
    }

    /// The files of the messages in the window, each with the host's path to it.
    fn context_attachments(
        &self,
        conversation_id: &str,
        linked: HashMap<String, Vec<LinkedFile>>,
    ) -> Result<HashMap<String, Vec<ContextAttachment>>, ChatError> {
        if linked.is_empty() {
            return Ok(HashMap::new());
        }
        let files = self
            .attachment_files()
            .ok_or(ChatError::Inconsistent("attachments root is not set"))?;
        linked
            .into_iter()
            .map(|(message_id, group)| -> Result<_, ChatError> {
                let attached = group
                    .into_iter()
                    .map(|file| {
                        let path = files
                            .file_path(conversation_id, &file.id)
                            .map_err(|_| ChatError::Inconsistent("attachment id"))?;
                        Ok(ContextAttachment { file, path })
                    })
                    .collect::<Result<Vec<_>, ChatError>>()?;
                Ok((message_id, attached))
            })
            .collect()
    }
}
