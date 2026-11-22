use crate::error::ChatError;
use crate::model::MessagePage;

use super::super::conversation_exists;
use super::timeline::{load_asc, load_desc};

impl super::super::ChatStore {
    pub fn messages(
        &self,
        conversation_id: &str,
        after_seq: Option<i64>,
        before_seq: Option<i64>,
        limit: usize,
    ) -> Result<MessagePage, ChatError> {
        let connection = self.lock();
        if !conversation_exists(&connection, conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        let (messages, next_before_seq) = match (after_seq, before_seq) {
            (Some(after), before) => (
                load_asc(&connection, conversation_id, after, before, limit)?,
                None,
            ),
            (None, before) => {
                let mut rows = load_desc(&connection, conversation_id, before, limit + 1)?;
                let has_more = rows.len() > limit;
                rows.truncate(limit);
                rows.reverse();
                let next = if has_more {
                    rows.first().map(|message| message.seq)
                } else {
                    None
                };
                (rows, next)
            }
        };
        Ok(MessagePage {
            messages,
            next_before_seq,
        })
    }
}
