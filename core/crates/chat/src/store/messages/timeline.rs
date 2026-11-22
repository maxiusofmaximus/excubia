use rusqlite::{params, Connection};

use crate::error::ChatError;
use crate::model::{Message, TimelineEntry};

use super::super::attachments::linked_files;
use super::rows::{message_row, timeline_message_row};

/// The message rows of a timeline page, with what only the timeline shows: the
/// quick replies, the turn state of a user message, the message it answers and
/// its files. The quote is read here, from the message itself, so it never goes stale.
pub(in crate::store) fn load_timeline_messages(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ChatError> {
    // A scalar subquery, not a join: a synthesis run also points at an agent
    // message, and a join would repeat that row.
    let mut statement = connection.prepare(
        "SELECT m.id, m.seq, m.author_kind, m.author_agent_id, m.body_md, m.state,
                m.created_at, m.suggestions_json,
                CASE WHEN m.author_kind = 'user'
                     THEN (SELECT r.state FROM run r WHERE r.message_id = m.id LIMIT 1) END,
                q.id, q.author_kind, q.body_md
           FROM message m
           LEFT JOIN message q ON q.id = m.reply_to_id
                AND m.reply_to_kind = 'message' AND q.conversation_id = m.conversation_id
          WHERE m.conversation_id = ?1 AND m.seq < ?2
          ORDER BY m.seq DESC LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![
            conversation_id,
            before_seq.unwrap_or(i64::MAX),
            limit as i64
        ],
        |row| timeline_message_row(row, conversation_id),
    )?;
    let mut entries: Vec<TimelineEntry> = rows.collect::<Result<_, _>>()?;
    let ids: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.message_id.as_deref())
        .collect();
    let mut files = linked_files(connection, &ids)?;
    for entry in &mut entries {
        entry.attachments = entry
            .message_id
            .as_deref()
            .and_then(|id| files.remove(id))
            .unwrap_or_default();
    }
    Ok(entries)
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

pub(super) fn load_asc(
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
