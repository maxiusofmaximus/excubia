use rusqlite::types::Type;
use rusqlite::Row;

use crate::model::{quote_preview, Message, Quote, RunState, TimelineEntry};

pub(super) fn timeline_message_row(
    row: &Row<'_>,
    conversation_id: &str,
) -> rusqlite::Result<TimelineEntry> {
    let mut entry = TimelineEntry::message(&message_row(row, conversation_id)?);
    if let Some(json) = row.get::<_, Option<String>>(7)? {
        entry.suggestions = serde_json::from_str(&json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(7, Type::Text, Box::new(error))
        })?;
    }
    entry.turn_state = row
        .get::<_, Option<String>>(8)?
        .as_deref()
        .and_then(RunState::from_db)
        .map(|state| state.as_str().to_owned());
    if let Some(id) = row.get::<_, Option<String>>(9)? {
        entry.quote = Some(Quote {
            kind: "message",
            id,
            author_kind: row.get(10)?,
            preview: quote_preview(&row.get::<_, String>(11)?),
        });
    }
    Ok(entry)
}

pub(super) fn message_row(row: &Row<'_>, conversation_id: &str) -> rusqlite::Result<Message> {
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
