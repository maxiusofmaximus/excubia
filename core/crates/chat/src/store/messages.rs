//! Messages: the idempotent user turn, the agent reply, the timeline and the
//! read cursor.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::ChatError;
use crate::id;
use crate::model::{preview, AcceptedMessage, Message, MessagePage, RunState, Turn};
use crate::schema;

use super::{alfred_id, append_event, conversation_exists, next_seq, turn_for_message};

impl super::ChatStore {
    /// Accept a user message. `client_id` is the idempotency key: replaying it
    /// returns the first message and its turn, and the caller must not deliver
    /// that turn again.
    /// Take back a turn the seat refused (`seat_unavailable`, `agent_busy`): the
    /// message and its run leave the conversation, as if never sent, so the
    /// client can send the same text again with the same `client_id`.
    pub fn withdraw_turn(&self, message_id: &str) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let conversation_id: String = transaction.query_row(
            "SELECT conversation_id FROM message WHERE id = ?1",
            [message_id],
            |row| row.get(0),
        )?;
        transaction.execute("DELETE FROM run WHERE message_id = ?1", [message_id])?;
        transaction.execute("DELETE FROM message WHERE id = ?1", [message_id])?;
        self.reindex(&transaction, &conversation_id)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn append_user_message(
        &self,
        conversation_id: &str,
        client_id: &str,
        text: &str,
    ) -> Result<AcceptedMessage, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let archived: Option<i64> = transaction
            .query_row(
                "SELECT archived FROM conversation WHERE id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(archived) = archived else {
            return Err(ChatError::NoSuchConversation);
        };
        if archived != 0 {
            return Err(ChatError::ConversationArchived);
        }

        if let Some(duplicate) = duplicate(&transaction, conversation_id, client_id)? {
            let body = duplicate.body_md.clone();
            let turn = turn_for_message(&transaction, &duplicate.id, conversation_id, &body)?;
            return Ok(AcceptedMessage {
                message: duplicate.into_message(conversation_id),
                turn,
                duplicate: true,
            });
        }

        let now = schema::now(&transaction)?;
        let seq = next_seq(&transaction, conversation_id)?;
        let message_id = id::new_id("msg");
        transaction.execute(
            "INSERT INTO message
                (id, conversation_id, seq, author_kind, author_agent_id,
                 body_md, state, client_id, created_at)
             VALUES (?1, ?2, ?3, 'user', NULL, ?4, 'complete', ?5, ?6)",
            params![message_id, conversation_id, seq, text, client_id, now],
        )?;
        transaction.execute(
            "UPDATE conversation SET last_message_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;
        self.reindex(&transaction, conversation_id)?;

        let agent_id = alfred_id(&transaction)?;
        let run_id = id::new_id("run");
        transaction.execute(
            "INSERT INTO run
                (id, agent_id, pane_id, message_id, state, started_at)
             VALUES (?1, ?2, NULL, ?3, 'queued', ?4)",
            params![run_id, agent_id, message_id, now],
        )?;
        let frame = append_event(
            &transaction,
            "chat.message",
            conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "created",
                "author_kind": "user",
                "preview": preview(text),
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        Ok(AcceptedMessage {
            message: Message {
                id: message_id.clone(),
                conversation_id: conversation_id.to_string(),
                seq,
                author_kind: "user".to_string(),
                author_agent_id: None,
                body_md: text.to_string(),
                state: "complete".to_string(),
                created_at: now,
            },
            turn: Turn {
                id: run_id,
                state: RunState::Queued,
                agent_id,
                conversation_id: conversation_id.to_string(),
                message_id,
                text: text.to_string(),
            },
            duplicate: false,
        })
    }

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

    /// Move the read cursor forward, never backwards, and never past the last
    /// message (`seen` semantics: non-destructive).
    pub fn mark_read(&self, conversation_id: &str, last_seq: i64) -> Result<i64, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current: Option<i64> = transaction
            .query_row(
                "SELECT last_read_seq FROM conversation WHERE id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Err(ChatError::NoSuchConversation);
        };
        let max: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM (
                 SELECT seq FROM message WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_card WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_approval WHERE conversation_id = ?1
             )",
            [conversation_id],
            |row| row.get(0),
        )?;
        let next = current.max(last_seq.clamp(0, max));
        if next != current {
            transaction.execute(
                "UPDATE conversation SET last_read_seq = ?1 WHERE id = ?2",
                params![next, conversation_id],
            )?;
        }
        transaction.commit()?;
        Ok(next)
    }
}

impl super::ChatStore {
    /// Insert one message and keep the conversation and search index in sync.
    /// Callers own the transaction and append the journal frames.
    pub(super) fn insert_message(
        &self,
        connection: &Connection,
        conversation_id: &str,
        author_kind: &str,
        author_agent_id: Option<&str>,
        body: &str,
        state: &str,
    ) -> Result<(String, i64, String), ChatError> {
        let now = schema::now(connection)?;
        let seq = next_seq(connection, conversation_id)?;
        let message_id = id::new_id("msg");
        connection.execute(
            "INSERT INTO message
                (id, conversation_id, seq, author_kind, author_agent_id,
                 body_md, state, client_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![
                message_id,
                conversation_id,
                seq,
                author_kind,
                author_agent_id,
                body,
                state,
                now
            ],
        )?;
        connection.execute(
            "UPDATE conversation SET last_message_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;
        self.reindex(connection, conversation_id)?;
        Ok((message_id, seq, now))
    }
}

pub(super) fn load_desc(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<Message>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at
               FROM message WHERE conversation_id = ?1 AND seq < ?2
              ORDER BY seq DESC LIMIT ?3"
        }
        None => {
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at
               FROM message WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows = match before_seq {
        Some(before) => statement
            .query_map(params![conversation_id, before, limit as i64], |row| {
                message_row(row, conversation_id)
            })?
            .collect::<Result<Vec<_>, _>>()?,
        None => statement
            .query_map(params![conversation_id, limit as i64], |row| {
                message_row(row, conversation_id)
            })?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

fn load_asc(
    connection: &Connection,
    conversation_id: &str,
    after_seq: i64,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<Message>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at
               FROM message WHERE conversation_id = ?1 AND seq > ?2 AND seq < ?3
              ORDER BY seq ASC LIMIT ?4"
        }
        None => {
            "SELECT id, seq, author_kind, author_agent_id, body_md, state, created_at
               FROM message WHERE conversation_id = ?1 AND seq > ?2
              ORDER BY seq ASC LIMIT ?3"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows = match before_seq {
        Some(before) => statement
            .query_map(
                params![conversation_id, after_seq, before, limit as i64],
                |row| message_row(row, conversation_id),
            )?
            .collect::<Result<Vec<_>, _>>()?,
        None => statement
            .query_map(params![conversation_id, after_seq, limit as i64], |row| {
                message_row(row, conversation_id)
            })?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

struct Duplicate {
    id: String,
    seq: i64,
    state: String,
    body_md: String,
    created_at: String,
}

impl Duplicate {
    fn into_message(self, conversation_id: &str) -> Message {
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

fn duplicate(
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

fn message_row(row: &Row<'_>, conversation_id: &str) -> rusqlite::Result<Message> {
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
}
