//! The chat event journal: the revision counter, the journal row of every event,
//! and the frames that tell the clients what a run's state means to them.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ChatError;
use crate::model::{ChatFrame, RunState};
use crate::schema;

pub(crate) fn revision(connection: &Connection) -> Result<i64, ChatError> {
    match schema::meta_value(connection, "revision")? {
        None => Ok(0),
        Some(value) => value
            .parse()
            .map_err(|_| ChatError::Inconsistent("chat revision is not a number")),
    }
}

/// Reserve the next global chat revision and write its journal row. The caller
/// must already hold a transaction: revision and mutation commit together.
pub(crate) fn append_event(
    connection: &Connection,
    event: &str,
    conversation_id: &str,
    body: serde_json::Value,
) -> Result<ChatFrame, ChatError> {
    append_journal(connection, event, Some(conversation_id), body)
}

/// [`append_event`] for a row that may name no conversation: the delete of one
/// keeps no row of it, its frame carries the id in the body only.
pub(crate) fn append_journal(
    connection: &Connection,
    event: &str,
    conversation_id: Option<&str>,
    mut body: serde_json::Value,
) -> Result<ChatFrame, ChatError> {
    let next = revision(connection)? + 1;
    schema::set_meta(connection, "revision", &next.to_string())?;
    body["revision"] = serde_json::Value::from(next);
    connection.execute(
        "INSERT INTO chat_event (id, event, conversation_id, body_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            next,
            event,
            conversation_id,
            body.to_string(),
            schema::now(connection)?
        ],
    )?;
    Ok(ChatFrame {
        id: next,
        event: event.to_string(),
        body,
    })
}

/// Journal what a run's state means to its clients: the `updated` frame of its
/// user message with the turn state the run has now, and the clearing of its
/// progress line once the run is closed. Runs that answer no user message and
/// show no progress (synthesis, delegations) journal nothing.
pub(crate) fn journal_turns<'a>(
    connection: &Connection,
    run_ids: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<ChatFrame>, ChatError> {
    let mut frames = Vec::new();
    for run_id in run_ids {
        frames.extend(clear_progress(connection, run_id)?);
        let turn: Option<(String, String, i64, String)> = connection
            .query_row(
                "SELECT m.id, m.conversation_id, m.seq, r.state
                   FROM run r JOIN message m ON m.id = r.message_id
                  WHERE r.id = ?1 AND m.author_kind = 'user'",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((message_id, conversation_id, seq, state)) = turn else {
            continue;
        };
        let turn_state =
            RunState::from_db(&state).ok_or(ChatError::Inconsistent("unknown run state"))?;
        frames.push(append_event(
            connection,
            "chat.message",
            &conversation_id,
            serde_json::json!({
                "event": "chat.message",
                "conversation_id": conversation_id,
                "message_id": message_id,
                "seq": seq,
                "change": "updated",
                "author_kind": "user",
                "turn_state": turn_state.as_str(),
            }),
        )?);
    }
    Ok(frames)
}

/// A closed run drops its progress line: the frame tells the clients to clear
/// the line they show. A run with no line journals nothing.
fn clear_progress(connection: &Connection, run_id: &str) -> Result<Option<ChatFrame>, ChatError> {
    let closed: Option<String> = connection
        .query_row(
            "SELECT m.conversation_id FROM run r JOIN message m ON m.id = r.message_id
              WHERE r.id = ?1 AND r.progress IS NOT NULL
                AND r.state NOT IN ('queued', 'running')",
            [run_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(conversation_id) = closed else {
        return Ok(None);
    };
    connection.execute(
        "UPDATE run SET progress = NULL, progress_at = NULL WHERE id = ?1",
        [run_id],
    )?;
    append_event(
        connection,
        "chat.progress",
        &conversation_id,
        serde_json::json!({
            "event": "chat.progress",
            "conversation_id": conversation_id,
            "run_id": run_id,
            "text": null,
        }),
    )
    .map(Some)
}
