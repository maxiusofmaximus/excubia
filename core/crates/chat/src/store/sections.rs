//! User sections: create and rename. "Recientes" is a query, never a table.

use rusqlite::{params, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model::Section;
use crate::schema;

use super::company_id;

impl super::ChatStore {
    pub fn sections(&self) -> Result<Vec<Section>, ChatError> {
        let connection = self.lock();
        let company = company_id(&connection)?;
        let mut statement = connection.prepare(
            "SELECT id, name, position FROM section
              WHERE company_id = ?1 AND archived = 0
              ORDER BY position ASC, id ASC",
        )?;
        let rows = statement.query_map([company], |row| {
            Ok(Section {
                id: row.get(0)?,
                name: row.get(1)?,
                order: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Create a section (no `id`) or rename one (with `id`).
    pub fn upsert_section(&self, id: Option<&str>, name: &str) -> Result<Section, ChatError> {
        let connection = self.lock();
        let company = company_id(&connection)?;
        if let Some(id) = id {
            let current: Option<(String, i64)> = connection
                .query_row(
                    "SELECT name, position FROM section
                      WHERE id = ?1 AND company_id = ?2 AND archived = 0",
                    params![id, company],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((current_name, position)) = current else {
                return Err(ChatError::UnknownSection(id.to_string()));
            };
            if current_name != name {
                let now = schema::now(&connection)?;
                connection.execute(
                    "UPDATE section SET name = ?1, updated_at = ?2 WHERE id = ?3",
                    params![name, now, id],
                )?;
            }
            return Ok(Section {
                id: id.to_string(),
                name: name.to_string(),
                order: position,
            });
        }
        let position: i64 = connection.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM section WHERE company_id = ?1",
            [&company],
            |row| row.get(0),
        )?;
        let id = id::new_id("sec");
        let now = schema::now(&connection)?;
        connection.execute(
            "INSERT INTO section
                (id, company_id, name, position, archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
            params![id, company, name, position, now],
        )?;
        Ok(Section {
            id,
            name: name.to_string(),
            order: position,
        })
    }
}
