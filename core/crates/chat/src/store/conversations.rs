//! Conversation listing, creation and metadata updates.

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};

use crate::error::ChatError;
use crate::id;
use crate::model::{
    Conversation, ConversationDetail, ConversationFilter, ConversationPage, ConversationUpdate,
    NewConversation, SeatSummary, SectionChange,
};
use crate::schema;

use super::{append_event, company_id, seat_summary};

const SELECT: &str = "SELECT c.id, c.title, c.project_id, c.pinned, c.section_id,
        c.archived, c.last_message_at, c.last_read_seq, c.created_at, c.updated_at,
        COALESCE((SELECT MAX(m.seq) FROM message m WHERE m.conversation_id = c.id), 0) AS last_seq,
        (SELECT COUNT(*) FROM message m
          WHERE m.conversation_id = c.id AND m.author_kind = 'agent'
            AND m.seq > c.last_read_seq) AS unread
   FROM conversation c";

impl super::ChatStore {
    pub fn list_conversations(
        &self,
        filter: ConversationFilter,
    ) -> Result<ConversationPage, ChatError> {
        let connection = self.lock();
        let company = company_id(&connection)?;
        let mut sql = format!("{SELECT} WHERE c.company_id = ?");
        let mut values = vec![Value::Text(company)];
        let archived = filter.archived as i64;
        sql.push_str(" AND c.archived = ?");
        values.push(Value::Integer(archived));
        if filter.pinned {
            sql.push_str(" AND c.pinned = 1");
        }
        if let Some(section_id) = &filter.section_id {
            sql.push_str(" AND c.section_id = ?");
            values.push(Value::Text(section_id.clone()));
        }
        if let Some(raw) = filter.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            match (self.search, fts_terms(raw)) {
                (schema::SearchMode::Fts5, Some(terms)) => {
                    sql.push_str(
                        " AND c.id IN (SELECT conversation_id FROM conversation_fts
                                        WHERE conversation_fts MATCH ?)",
                    );
                    values.push(Value::Text(terms));
                }
                _ => {
                    let pattern = like_pattern(raw);
                    sql.push_str(
                        " AND (c.title LIKE ? ESCAPE '\\' OR EXISTS (
                              SELECT 1 FROM message m
                               WHERE m.conversation_id = c.id
                                 AND m.body_md LIKE ? ESCAPE '\\'))",
                    );
                    values.push(Value::Text(pattern.clone()));
                    values.push(Value::Text(pattern));
                }
            }
        }
        if let Some(cursor) = &filter.cursor {
            let (pinned, last_message_at) = cursor_position(&connection, cursor)?;
            sql.push_str(
                " AND (c.pinned < ? OR (c.pinned = ? AND
                   (c.last_message_at < ? OR (c.last_message_at = ? AND c.id < ?))))",
            );
            values.push(Value::Integer(pinned));
            values.push(Value::Integer(pinned));
            values.push(Value::Text(last_message_at.clone()));
            values.push(Value::Text(last_message_at));
            values.push(Value::Text(cursor.clone()));
        }
        sql.push_str(" ORDER BY c.pinned DESC, c.last_message_at DESC, c.id DESC LIMIT ?");
        values.push(Value::Integer((filter.limit as i64).saturating_add(1)));

        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), row_to_conversation)?;
        let mut conversations = rows.collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if conversations.len() > filter.limit {
            conversations.truncate(filter.limit);
            conversations.last().map(|row| row.id.clone())
        } else {
            None
        };
        Ok(ConversationPage {
            conversations,
            next_cursor,
        })
    }

    pub fn create_conversation(&self, new: NewConversation) -> Result<Conversation, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let company = company_id(&transaction)?;
        if let Some(project_id) = &new.project_id {
            ensure_project(&transaction, project_id)?;
        }
        let project_id = new.project_id;
        let title = new
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Nuevo chat".to_string());
        let now = schema::now(&transaction)?;
        let id = id::new_id("cnv");
        transaction.execute(
            "INSERT INTO conversation
                (id, company_id, project_id, title, pinned, section_id, archived,
                 last_message_at, last_read_seq, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0, NULL, 0, ?5, 0, ?5, ?5)",
            params![&id, company, &project_id, &title, now],
        )?;
        self.reindex(&transaction, &id)?;
        let frame = append_event(
            &transaction,
            "chat.conversation",
            &id,
            serde_json::json!({
                "event": "chat.conversation",
                "conversation_id": &id,
                "change": "created",
            }),
        )?;
        let conversation = Conversation::empty(id, title, project_id, now);
        transaction.commit()?;
        self.publish(frame);
        Ok(conversation)
    }

    pub fn conversation(&self, conversation_id: &str) -> Result<ConversationDetail, ChatError> {
        let connection = self.lock();
        let conversation =
            conversation_row(&connection, conversation_id)?.ok_or(ChatError::NoSuchConversation)?;
        let mut participants = Vec::new();
        if let Some(alfred) = seat_summary(&connection, "alfred")? {
            participants.push(alfred);
        }
        if let Some(project_id) = &conversation.project_id {
            let lead: Option<SeatSummary> = connection
                .query_row(
                    "SELECT s.id, s.name, s.status, s.uac_state, s.pane_id
                       FROM project p JOIN agent_seat s ON s.id = p.lead_agent_id
                      WHERE p.id = ?1",
                    [project_id],
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
                .optional()?;
            if let Some(lead) = lead {
                if participants
                    .iter()
                    .all(|participant| participant.agent_id != lead.agent_id)
                {
                    participants.push(lead);
                }
            }
        }
        Ok(ConversationDetail {
            conversation,
            participants,
        })
    }

    pub fn update_conversation(
        &self,
        conversation_id: &str,
        update: ConversationUpdate,
    ) -> Result<Conversation, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = conversation_row(&transaction, conversation_id)?
            .ok_or(ChatError::NoSuchConversation)?;
        let title = update.title.unwrap_or_else(|| current.title.clone());
        let pinned = update.pinned.unwrap_or(current.pinned);
        let archived = update.archived.unwrap_or(current.archived);
        let section_id = match update.section {
            SectionChange::Keep => current.section_id.clone(),
            SectionChange::Clear => None,
            SectionChange::Set(section_id) => {
                if !section_exists(&transaction, &section_id)? {
                    return Err(ChatError::UnknownSection(section_id));
                }
                Some(section_id)
            }
        };
        let changed = title != current.title
            || pinned != current.pinned
            || archived != current.archived
            || section_id != current.section_id;
        if !changed {
            transaction.commit()?;
            return Ok(current);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE conversation
                SET title = ?1, pinned = ?2, section_id = ?3, archived = ?4, updated_at = ?5
              WHERE id = ?6",
            params![title, pinned, section_id, archived, now, conversation_id],
        )?;
        if title != current.title {
            self.reindex(&transaction, conversation_id)?;
        }
        let frame = append_event(
            &transaction,
            "chat.conversation",
            conversation_id,
            serde_json::json!({
                "event": "chat.conversation",
                "conversation_id": conversation_id,
                "change": "updated",
            }),
        )?;
        let updated = conversation_row(&transaction, conversation_id)?
            .ok_or(ChatError::NoSuchConversation)?;
        transaction.commit()?;
        self.publish(frame);
        Ok(updated)
    }
}

impl super::ChatStore {
    pub(super) fn reindex(
        &self,
        connection: &Connection,
        conversation_id: &str,
    ) -> Result<(), ChatError> {
        if self.search == schema::SearchMode::Fts5 {
            schema::reindex(connection, conversation_id)?;
        }
        Ok(())
    }
}

fn section_exists(connection: &Connection, section_id: &str) -> Result<bool, ChatError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM section WHERE id = ?1 AND archived = 0",
            [section_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

fn ensure_project(connection: &Connection, project_id: &str) -> Result<(), ChatError> {
    let exists: Option<i64> = connection
        .query_row("SELECT 1 FROM project WHERE id = ?1", [project_id], |row| {
            row.get(0)
        })
        .optional()?;
    if exists.is_some() {
        Ok(())
    } else {
        Err(ChatError::UnknownProject(project_id.to_string()))
    }
}

fn cursor_position(connection: &Connection, cursor: &str) -> Result<(i64, String), ChatError> {
    connection
        .query_row(
            "SELECT pinned, last_message_at FROM conversation WHERE id = ?1",
            [cursor],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| ChatError::InvalidCursor(cursor.to_string()))
}

pub(super) fn conversation_row(
    connection: &Connection,
    conversation_id: &str,
) -> Result<Option<Conversation>, ChatError> {
    let sql = format!("{SELECT} WHERE c.id = ?1");
    Ok(connection
        .query_row(&sql, [conversation_id], row_to_conversation)
        .optional()?)
}

fn row_to_conversation(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        title: row.get(1)?,
        project_id: row.get(2)?,
        pinned: row.get(3)?,
        section_id: row.get(4)?,
        archived: row.get(5)?,
        last_message_at: row.get(6)?,
        last_read_seq: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        last_seq: row.get(10)?,
        unread: row.get::<_, i64>(11)?.max(0) as u64,
    })
}

/// FTS5 syntax is a query language; a user's text must never become one. Split
/// it into words and quote each, so punctuation cannot produce an error.
fn fts_terms(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

fn like_pattern(raw: &str) -> String {
    let mut pattern = String::with_capacity(raw.len() + 2);
    pattern.push('%');
    for character in raw.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_input_is_tokenized_and_quoted() {
        assert_eq!(
            fts_terms("plan \"Demo\" - presupuesto"),
            Some("\"plan\" \"Demo\" \"presupuesto\"".to_string())
        );
        assert_eq!(fts_terms("..."), None);
    }

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(like_pattern("50%_x"), "%50\\%\\_x%");
    }
}
