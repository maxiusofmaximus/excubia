use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model::{preview, AcceptedMessage, Message, RunState, Turn};
use crate::schema;

use super::super::attachments::link_in;
use super::super::{alfred_id, append_event, next_seq, turn_for_message};
use super::duplicate::duplicate;

impl super::super::ChatStore {
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

    /// `reply_to` names the message this one answers, as `(kind, id)`; only
    /// `message` is replyable so far, and the id must be in this conversation.
    pub fn append_user_message(
        &self,
        conversation_id: &str,
        client_id: &str,
        text: &str,
        reply_to: Option<(&str, &str)>,
    ) -> Result<AcceptedMessage, ChatError> {
        self.append_user_message_with_attachments(conversation_id, client_id, text, reply_to, &[])
    }

    /// A user message that carries ready files. The message and its links are one
    /// transaction: a file the conversation cannot take refuses the whole message.
    pub fn append_user_message_with_attachments(
        &self,
        conversation_id: &str,
        client_id: &str,
        text: &str,
        reply_to: Option<(&str, &str)>,
        attachment_ids: &[&str],
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

        ensure_reply_target(&transaction, conversation_id, reply_to)?;
        let now = schema::now(&transaction)?;
        let seq = next_seq(&transaction, conversation_id)?;
        let message_id = id::new_id("msg");
        transaction.execute(
            "INSERT INTO message
                (id, conversation_id, seq, author_kind, author_agent_id,
                 body_md, state, client_id, created_at, reply_to_kind, reply_to_id)
             VALUES (?1, ?2, ?3, 'user', NULL, ?4, 'complete', ?5, ?6, ?7, ?8)",
            params![
                message_id,
                conversation_id,
                seq,
                text,
                client_id,
                now,
                reply_to.map(|(kind, _)| kind),
                reply_to.map(|(_, id)| id)
            ],
        )?;
        link_in(&transaction, conversation_id, &message_id, attachment_ids)?;
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

    /// A message Alfred posts with no turn behind it: the host's report of an action
    /// the operator decided (F2b-4). It is an agent message in the same journal as a reply.
    pub fn append_alfred_message(
        &self,
        conversation_id: &str,
        text: &str,
    ) -> Result<Message, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let agent_id = alfred_id(&transaction)?;
        let (message_id, seq, created_at) = self.insert_message(
            &transaction,
            conversation_id,
            "agent",
            Some(&agent_id),
            text,
            "complete",
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
                "author_kind": "agent",
                "preview": preview(text),
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        Ok(Message {
            id: message_id,
            conversation_id: conversation_id.to_string(),
            seq,
            author_kind: "agent".to_string(),
            author_agent_id: Some(agent_id),
            body_md: text.to_string(),
            state: "complete".to_string(),
            created_at,
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
                 SELECT seq FROM timeline_mail_card WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_mail_draft WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_approval WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_plan WHERE conversation_id = ?1
                 UNION ALL
                 SELECT seq FROM timeline_calendar_card WHERE conversation_id = ?1
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

impl super::super::ChatStore {
    /// Insert one message and keep the conversation and search index in sync.
    /// Callers own the transaction and append the journal frames.
    pub(in crate::store) fn insert_message(
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

/// A reply names a message of the same conversation. Cards are not replyable yet.
fn ensure_reply_target(
    connection: &Connection,
    conversation_id: &str,
    reply_to: Option<(&str, &str)>,
) -> Result<(), ChatError> {
    let Some((kind, id)) = reply_to else {
        return Ok(());
    };
    let found = kind == "message"
        && connection
            .query_row(
                "SELECT 1 FROM message WHERE id = ?1 AND conversation_id = ?2",
                params![id, conversation_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
    if found {
        Ok(())
    } else {
        Err(ChatError::InvalidReplyTo(id.to_string()))
    }
}
