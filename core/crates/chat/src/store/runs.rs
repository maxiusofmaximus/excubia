//! Turn lifecycle. CORE6B is the only writer of these states: the seat marks
//! `running` when it dequeues, `completed` after a validated `chat.reply`,
//! `failed` when the process ends without replying (or times out) and the host
//! marks a surviving `queued`/`running` row `interrupted` at startup.

use rusqlite::{params, Connection, OptionalExtension};

use crate::clock;
use crate::error::ChatError;
use crate::model::{preview, Message, RunState};
use crate::schema;

use super::agent_files::{announce_message, check_agent_files};
use super::attachments::link_in;
use super::{append_event, conversation_exists, journal_turns, open_run_ids, run_info};
use super::lookup::{RUN_CONVERSATION, RUN_MESSAGE_JOIN};

const MAX_SUGGESTIONS: usize = 4;
const MAX_SUGGESTION_CHARS: usize = 60;
/// A progress line that arrives this soon after the accepted one is held back.
const PROGRESS_WINDOW_MS: i64 = 5_000;
/// The longest reply that the host's assignment line replaces.
const MAX_ASSIGNMENT_REPLY_CHARS: usize = 600;

/// What the store did with a progress line: kept, or held back by the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressOutcome {
    Accepted,
    Throttled,
}

/// Up to four quick replies of 1 to 60 characters, without control characters.
/// Anything outside those limits is refused, never cut.
pub fn validate_suggestions(suggestions: &[String]) -> Result<(), ChatError> {
    if suggestions.len() > MAX_SUGGESTIONS {
        return Err(ChatError::InvalidSuggestions("at most 4 suggestions"));
    }
    for suggestion in suggestions {
        if !(1..=MAX_SUGGESTION_CHARS).contains(&suggestion.chars().count()) {
            return Err(ChatError::InvalidSuggestions(
                "each suggestion has 1 to 60 characters",
            ));
        }
        if suggestion.chars().any(char::is_control) {
            return Err(ChatError::InvalidSuggestions(
                "suggestions cannot contain control characters",
            ));
        }
    }
    Ok(())
}

/// Whether the operator denied an approval of the conversation after its last message
/// from the user (the turn's own denials included, since the turn opens with that message).
fn operator_denied_since_user_message(
    connection: &Connection,
    conversation_id: &str,
) -> Result<bool, ChatError> {
    let denied: i64 = connection.query_row(
        "SELECT COUNT(*) FROM approval
          WHERE conversation_id = ?1 AND status = 'denied' AND decider_kind = 'user'
            AND decided_at > COALESCE((
                SELECT MAX(created_at) FROM message
                 WHERE conversation_id = ?1 AND author_kind = 'user'), '')",
        [conversation_id],
        |row| row.get(0),
    )?;
    Ok(denied > 0)
}

/// The host's line for an Alfred turn that assigned work: «Pedido a {Nombre}: {título}.»,
/// one line per delegation. `None` when nothing was assigned, the reply asks a question
/// or it is longer than the line the host writes.
fn assignment_reply(
    connection: &Connection,
    run_id: &str,
    agent_id: &str,
    reply: &str,
) -> Result<Option<String>, ChatError> {
    if reply.contains('?') || reply.chars().count() > MAX_ASSIGNMENT_REPLY_CHARS {
        return Ok(None);
    }
    let mut statement = connection.prepare(
        "SELECT t.name, d.task FROM delegation d
           JOIN agent_seat t ON t.id = d.to_agent_id
          WHERE d.turn_key = ?1 AND d.from_agent_id = ?2
          ORDER BY d.rowid",
    )?;
    let lines = statement
        .query_map(params![run_id, agent_id], |row| {
            let name: String = row.get(0)?;
            let task: String = row.get(1)?;
            let title = crate::text::close_title(&task);
            let stop = if title.ends_with('…') { "" } else { "." };
            Ok(format!("Pedido a {name}: {title}{stop}"))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((!lines.is_empty()).then(|| lines.join("\n")))
}

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

    /// Whether the message a turn answers was written by the user. Alfred's own
    /// turns (a delegation's synthesis) answer a message of Alfred's.
    pub fn is_user_turn(&self, message_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let author: Option<String> = connection
            .query_row(
                "SELECT author_kind FROM message WHERE id = ?1",
                [message_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(author.as_deref() == Some("user"))
    }

    /// Dequeue one turn. Returns false when the run is already terminal (a
    /// restart or a duplicate delivery got there first).
    pub fn start_run(&self, run_id: &str) -> Result<bool, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE run SET state = 'running', started_at = ?1
              WHERE id = ?2 AND state = 'queued'",
            params![now, run_id],
        )?;
        if changed != 1 {
            return Ok(false);
        }
        let frames = journal_turns(&transaction, [run_id])?;
        transaction.commit()?;
        self.publish_all(frames);
        Ok(true)
    }

    /// Close a turn with the only valid response: the `chat.reply` text and its
    /// quick replies. The message insert and the run transition commit together,
    /// and the journal gets `created` then `updated` for the same message, then
    /// the `updated` of the user message that opened the turn.
    pub fn complete_run_with_reply(
        &self,
        run_id: &str,
        text: &str,
        suggestions: &[String],
    ) -> Result<Message, ChatError> {
        self.complete_run_with_files(run_id, text, suggestions, &[])
    }

    /// The same close, with the files Alfred made in this turn: `attachment_ids` are
    /// the ones the reply shows; the others follow in messages of their own.
    pub fn complete_run_with_files(
        &self,
        run_id: &str,
        text: &str,
        suggestions: &[String],
        attachment_ids: &[&str],
    ) -> Result<Message, ChatError> {
        self.complete_runs_with_files(&[run_id], text, suggestions, attachment_ids)
    }

    /// One reply closes a merged turn: every run of `run_ids` completes together, and
    /// the first one names the conversation. If any run is already closed, nothing is
    /// written and the reply is refused.
    pub fn complete_runs_with_files(
        &self,
        run_ids: &[&str],
        text: &str,
        suggestions: &[String],
        attachment_ids: &[&str],
    ) -> Result<Message, ChatError> {
        validate_suggestions(suggestions)?;
        let Some(&run_id) = run_ids.first() else {
            return Err(ChatError::Inconsistent("a turn without runs"));
        };
        let text = &crate::text::without_internal_ids(text);
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = run_info(&transaction, run_id)?;
        // A turn that only assigned work is closed with the host's line, not the model's.
        let host_line = assignment_reply(&transaction, run_id, &info.agent_id, text)?;
        let text: &str = host_line.as_deref().unwrap_or(text.as_str());
        // The operator refused an action here: the quick replies would offer it again.
        let refused = operator_denied_since_user_message(&transaction, &info.conversation_id)?;
        let suggestions: &[String] = if refused { &[] } else { suggestions };
        let now = schema::now(&transaction)?;
        let mut changed = 0;
        for id in run_ids {
            changed += transaction.execute(
                "UPDATE run SET state = 'completed', finished_at = ?1, reason = NULL
                  WHERE id = ?2 AND state IN ('queued', 'running')",
                params![now, id],
            )?;
        }
        if changed != run_ids.len() {
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
        if !suggestions.is_empty() {
            transaction.execute(
                "UPDATE message SET suggestions_json = ?1 WHERE id = ?2",
                params![serde_json::to_string(suggestions)?, message_id],
            )?;
        }
        check_agent_files(&transaction, &info.conversation_id, attachment_ids)?;
        link_in(
            &transaction,
            &info.conversation_id,
            &message_id,
            attachment_ids,
        )?;
        let mut frames =
            announce_message(&transaction, &info.conversation_id, &message_id, seq, text)?;
        frames.extend(self.deliver_unnamed_files(
            &transaction,
            &info.conversation_id,
            &info.agent_id,
            run_id,
        )?);
        let turn = journal_turns(&transaction, run_ids.iter().copied())?;
        transaction.commit()?;
        self.publish_all(frames);
        self.publish_all(turn);
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
        let text = &crate::text::without_internal_ids(text);
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

    /// Show what the run is doing now. A line inside the window of the last
    /// accepted one is not an error: the stored line stays and nothing is
    /// journaled. A closed run refuses it.
    pub fn set_run_progress(&self, run_id: &str, text: &str) -> Result<ProgressOutcome, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = run_info(&transaction, run_id)?;
        if !matches!(info.state, RunState::Queued | RunState::Running) {
            return Err(ChatError::RunNotActive(run_id.to_string()));
        }
        let last: Option<String> = transaction.query_row(
            "SELECT progress_at FROM run WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        let now = self.now_ms();
        if let Some(at) = last.as_deref().and_then(clock::parse_iso) {
            if (0..PROGRESS_WINDOW_MS).contains(&(now - at)) {
                return Ok(ProgressOutcome::Throttled);
            }
        }
        transaction.execute(
            "UPDATE run SET progress = ?1, progress_at = ?2 WHERE id = ?3",
            params![text, clock::iso(now), run_id],
        )?;
        let frame = append_event(
            &transaction,
            "chat.progress",
            &info.conversation_id,
            serde_json::json!({
                "event": "chat.progress",
                "conversation_id": info.conversation_id,
                "run_id": run_id,
                "text": text,
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        Ok(ProgressOutcome::Accepted)
    }

    /// The process ended without `chat.reply` (or hit the turn timeout). A
    /// failed, empty agent message is the card: clients render it as "the
    /// agent ended without answering", never as invented text.
    pub fn fail_run_with_card(&self, run_id: &str, reason: &str) -> Result<bool, ChatError> {
        self.fail_runs_with_card(&[run_id], reason)
    }

    /// A merged turn that failed: every run of `run_ids` fails and the conversation
    /// gets one failed card. Runs closed earlier (a stop) keep their state.
    pub fn fail_runs_with_card(&self, run_ids: &[&str], reason: &str) -> Result<bool, ChatError> {
        let Some(&run_id) = run_ids.first() else {
            return Ok(false);
        };
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let info = match run_info(&transaction, run_id) {
            Ok(info) => info,
            Err(ChatError::NoSuchRun(_)) => return Ok(false),
            Err(error) => return Err(error),
        };
        let now = schema::now(&transaction)?;
        let mut changed = 0;
        for id in run_ids {
            changed += transaction.execute(
                "UPDATE run SET state = 'failed', finished_at = ?1, reason = ?2
                  WHERE id = ?3 AND state IN ('queued', 'running')",
                params![now, reason, id],
            )?;
        }
        if changed == 0 {
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
        let turn = journal_turns(&transaction, run_ids.iter().copied())?;
        transaction.commit()?;
        self.publish(created);
        self.publish(updated);
        self.publish_all(turn);
        Ok(true)
    }

    /// Graceful shutdown: no queued turn survives the process that owns the
    /// queue. The same rule runs at startup for a crash (`host_restart`).
    pub fn interrupt_active(&self, reason: &str) -> Result<usize, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        let run_ids = open_run_ids(&transaction)?;
        let changed = transaction.execute(
            "UPDATE run SET state = 'interrupted', finished_at = ?1, reason = ?2
              WHERE state IN ('queued', 'running')",
            params![now, reason],
        )?;
        let frames = journal_turns(&transaction, run_ids.iter().map(String::as_str))?;
        // The hook tokens of those runs die with them.
        transaction.execute(
            "UPDATE hook_token SET revoked_at = ?1 WHERE revoked_at IS NULL",
            [&now],
        )?;
        transaction.commit()?;
        self.publish_all(frames);
        Ok(changed)
    }

    /// The user stops a conversation's open turns. Every `queued` or `running`
    /// run of its messages becomes `interrupted` with `reason`, and each user
    /// message journals its new turn state. Nothing is deleted; a run that is
    /// already terminal is not touched, so a second stop returns no runs.
    pub fn stop_conversation_runs(
        &self,
        conversation_id: &str,
        reason: &str,
    ) -> Result<Vec<String>, ChatError> {
        let mut connection = self.lock();
        if !conversation_exists(&connection, conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        let transaction = connection.transaction()?;
        let run_ids: Vec<String> = {
            let mut statement = transaction.prepare(&format!(
                "SELECT r.id FROM run r {RUN_MESSAGE_JOIN}
                  WHERE r.state IN ('queued', 'running') AND {RUN_CONVERSATION} = ?1"
            ))?;
            let ids = statement
                .query_map([conversation_id], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            ids
        };
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE run SET state = 'interrupted', finished_at = ?1, reason = ?2
              WHERE state IN ('queued', 'running') AND (message_id IN (
                  SELECT id FROM message WHERE conversation_id = ?3)
                  OR id IN (SELECT run_id FROM plan_resume
                             WHERE conversation_id = ?3 AND run_id IS NOT NULL))",
            params![now, reason, conversation_id],
        )?;
        super::plans::interrupt_plan_runs(&transaction, conversation_id, reason)?;
        let frames = journal_turns(&transaction, run_ids.iter().map(String::as_str))?;
        transaction.commit()?;
        self.publish_all(frames);
        Ok(run_ids)
    }

    /// Refuse a turn before a process ever runs (no CLI, full queue). No card:
    /// the client gets the `409` and decides whether to retry.
    pub fn fail_run(&self, run_id: &str, reason: &str) -> Result<bool, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        let changed = transaction.execute(
            "UPDATE run SET state = 'failed', finished_at = ?1, reason = ?2
              WHERE id = ?3 AND state IN ('queued', 'running')",
            params![now, reason, run_id],
        )?;
        let frames = if changed == 1 {
            journal_turns(&transaction, [run_id])?
        } else {
            Vec::new()
        };
        transaction.commit()?;
        self.publish_all(frames);
        Ok(changed == 1)
    }
}
