//! Row lookups the chat store and its handlers share: the company and Alfred's
//! seat, the runs of a message, and the next sequence number of a conversation.

use rusqlite::{Connection, OptionalExtension};

use crate::error::ChatError;
use crate::model::{RunState, SeatSummary, Turn};

/// The runs that are still `queued` or `running`.
pub(crate) fn open_run_ids(connection: &Connection) -> Result<Vec<String>, ChatError> {
    let mut statement =
        connection.prepare("SELECT id FROM run WHERE state IN ('queued', 'running')")?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

pub(crate) fn company_id(connection: &Connection) -> Result<String, ChatError> {
    connection
        .query_row("SELECT id FROM company LIMIT 1", [], |row| row.get(0))
        .optional()?
        .ok_or(ChatError::NotInitialised)
}

pub(crate) fn alfred_id(connection: &Connection) -> Result<String, ChatError> {
    connection
        .query_row(
            "SELECT id FROM agent_seat WHERE role_id = 'alfred' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(ChatError::NotInitialised)
}

pub(crate) fn seat_summary(
    connection: &Connection,
    role_id: &str,
) -> Result<Option<SeatSummary>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, name, status, uac_state, pane_id FROM agent_seat
              WHERE role_id = ?1 ORDER BY created_at ASC, id ASC LIMIT 1",
            [role_id],
            |row| {
                Ok(SeatSummary {
                    agent_id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    uac_state: row.get(3)?,
                    pane_id: row.get(4)?,
                })
            },
        )
        .optional()?)
}

pub(crate) fn conversation_exists(
    connection: &Connection,
    conversation_id: &str,
) -> Result<bool, ChatError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM conversation WHERE id = ?1",
            [conversation_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

pub(crate) fn next_seq(connection: &Connection, conversation_id: &str) -> Result<i64, ChatError> {
    Ok(connection.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM (
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
             SELECT seq FROM timeline_calendar_card WHERE conversation_id = ?1
         )",
        [conversation_id],
        |row| row.get(0),
    )?)
}

#[derive(Clone, Debug)]
pub(crate) struct RunInfo {
    pub agent_id: String,
    pub conversation_id: String,
    pub state: RunState,
}

pub(crate) fn run_info(connection: &Connection, run_id: &str) -> Result<RunInfo, ChatError> {
    connection
        .query_row(
            "SELECT r.agent_id, m.conversation_id, r.state
               FROM run r JOIN message m ON m.id = r.message_id
              WHERE r.id = ?1",
            [run_id],
            |row| {
                Ok(RunInfo {
                    agent_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    state: RunState::from_db(&row.get::<_, String>(2)?)
                        .unwrap_or(RunState::Interrupted),
                })
            },
        )
        .optional()?
        .ok_or_else(|| ChatError::NoSuchRun(run_id.to_string()))
}

pub(crate) fn turn_for_message(
    connection: &Connection,
    message_id: &str,
    conversation_id: &str,
    text: &str,
) -> Result<Turn, ChatError> {
    let (id, state, agent_id): (String, String, String) = connection
        .query_row(
            "SELECT id, state, agent_id FROM run WHERE message_id = ?1
              ORDER BY rowid DESC LIMIT 1",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ChatError::Inconsistent("message without a turn"))?;
    Ok(Turn {
        id,
        state: RunState::from_db(&state).unwrap_or(RunState::Interrupted),
        agent_id,
        conversation_id: conversation_id.to_string(),
        message_id: message_id.to_string(),
        text: text.to_string(),
    })
}
