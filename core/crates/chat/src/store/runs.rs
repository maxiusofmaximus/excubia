//! Turn lifecycle. CORE6B is the only writer of these states: the seat marks
//! `running` when it dequeues, `completed` after a validated `chat.reply`,
//! `failed` when the process ends without replying (or times out) and the host
//! marks a surviving `queued`/`running` row `interrupted` at startup.

use rusqlite::{params, OptionalExtension};

use crate::error::ChatError;
use crate::model::{preview, Message, RunState};
use crate::schema;

use super::{append_event, run_info};

impl super::ChatStore {
    pub fn run_state(&self, run_id: &str) -> Result<Option<RunState>, ChatError> {
        let connection = self.lock();
        let state: Option<String> = connection
            .query_row("SELECT state FROM run WHERE id = ?1", [run_id], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(state.as_deref().and_then(RunState::from_db))
    }

    /// Dequeue one turn. Returns false when the run is already terminal (a
    /// restart or a duplicate delivery got there first).
    pub fn start_run(&self, run_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let now = schema::now(&connection)?;
        let changed = connection.execute(
            "UPDATE run SET state = 'running', started_at = ?1
              WHERE id = ?2 AND state = 'queued'",
            params![now, run_id],
        )?;
        Ok(changed == 1)
    }

    /// Close a turn with the only valid response: the `chat.reply` text. The
    /// message insert and the run transition commit together, and the journal
    /// gets `created` then `updated` for the same message.
    pub fn complete_run_with_reply(&self, run_id: &str, text: &str) -> Result<Message, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = run_info(&transaction, run_id)?;
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE run SET state = 'completed', finished_at = ?1, reason = NULL
              WHERE id = ?2 AND state IN ('queued', 'running')",
            params![now, run_id],
        )?;
        if changed != 1 {
            return Err(ChatError::RunNotActive(run_id.to_string()));
        }
        let (message_id, seq, created_at) = self.insert_message(
            &transaction,
            &info.conversation_id,
            "agent",
            Some(&info.agent_id),
            text,
            "complete",
        )?;
        let body = serde_json::json!({
            "event": "chat.message",
            "conversation_id": info.conversation_id,
            "message_id": message_id,
            "seq": seq,
            "change": "created",
            "author_kind": "agent",
            "preview": preview(text),
        });
        let created = append_event(&transaction, "chat.message", &info.conversation_id, body)?;
        let updated = append_event(
            &transaction,
            "chat.message",
            &info.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": info.conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "updated",
                "author_kind": "agent",
                "state": "complete",
                "preview": preview(text),
            }),
        )?;
        transaction.commit()?;
        self.publish(created);
        self.publish(updated);
        Ok(Message {
            id: message_id,
            conversation_id: info.conversation_id,
            seq,
            author_kind: "agent".to_string(),
            author_agent_id: Some(info.agent_id),
            body_md: text.to_string(),
            state: "complete".to_string(),
            created_at,
        })
    }

    /// A question from `chat.ask_user`: it enters the timeline as agent data,
    /// but it is not a reply and does not close the run.
    pub fn append_agent_question(&self, run_id: &str, text: &str) -> Result<Message, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = run_info(&transaction, run_id)?;
        if !matches!(info.state, RunState::Queued | RunState::Running) {
            return Err(ChatError::RunNotActive(run_id.to_string()));
        }
        let (message_id, seq, created_at) = self.insert_message(
            &transaction,
            &info.conversation_id,
            "agent",
            Some(&info.agent_id),
            text,
            "complete",
        )?;
        let frame = append_event(
            &transaction,
            "chat.message",
            &info.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": info.conversation_id,
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
            conversation_id: info.conversation_id,
            seq,
            author_kind: "agent".to_string(),
            author_agent_id: Some(info.agent_id),
            body_md: text.to_string(),
            state: "complete".to_string(),
            created_at,
        })
    }

    /// The process ended without `chat.reply` (or hit the turn timeout). A
    /// failed, empty agent message is the card: clients render it as "the
    /// agent ended without answering", never as invented text.
    pub fn fail_run_with_card(&self, run_id: &str, reason: &str) -> Result<bool, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = match run_info(&transaction, run_id) {
            Ok(info) => info,
            Err(ChatError::NoSuchRun(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE run SET state = 'failed', finished_at = ?1, reason = ?2
              WHERE id = ?3 AND state IN ('queued', 'running')",
            params![now, reason, run_id],
        )?;
        if changed != 1 {
            return Ok(false);
        }
        let (message_id, seq, _created_at) = self.insert_message(
            &transaction,
            &info.conversation_id,
            "agent",
            Some(&info.agent_id),
            "",
            "failed",
        )?;
        let created = append_event(
            &transaction,
            "chat.message",
            &info.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": info.conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "created",
                "author_kind": "agent",
                "preview": "",
            }),
        )?;
        let updated = append_event(
            &transaction,
            "chat.message",
            &info.conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": info.conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "updated",
                "author_kind": "agent",
                "state": "failed",
                "preview": "",
            }),
        )?;
        transaction.commit()?;
        self.publish(created);
        self.publish(updated);
        Ok(true)
    }

    /// Graceful shutdown: no queued turn survives the process that owns the
    /// queue. The same rule runs at startup for a crash (`host_restart`).
    pub fn interrupt_active(&self, reason: &str) -> Result<usize, ChatError> {
        let connection = self.lock();
        let now = schema::now(&connection)?;
        let changed = connection.execute(
            "UPDATE run SET state = 'interrupted', finished_at = ?1, reason = ?2
              WHERE state IN ('queued', 'running')",
            params![now, reason],
        )?;
        // The hook tokens of those runs die with them.
        connection.execute(
            "UPDATE hook_token SET revoked_at = ?1 WHERE revoked_at IS NULL",
            [&now],
        )?;
        Ok(changed)
    }

    /// Refuse a turn before a process ever runs (no CLI, full queue). No card:
    /// the client gets the `409` and decides whether to retry.
    pub fn fail_run(&self, run_id: &str, reason: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let now = schema::now(&connection)?;
        let changed = connection.execute(
            "UPDATE run SET state = 'failed', finished_at = ?1, reason = ?2
              WHERE id = ?3 AND state IN ('queued', 'running')",
            params![now, reason, run_id],
        )?;
        Ok(changed == 1)
    }
}
