//! Finding contacts (F3, AGENDA-REST): the search Alfred runs, and the newest-first list
//! the app pages through. Both match a name or an alias, folded the way a search folds it.

use rusqlite::{Connection, OptionalExtension};

use super::agenda::older_than;
use super::contacts::{aliases_of, contact_by_id, contact_rows};
use crate::error::ChatError;
use crate::model_contact::{normalize_text, Contact, CONTACT_FIND_MAX_RESULTS};

impl super::ChatStore {
    /// The contacts a search finds, at most `CONTACT_FIND_MAX_RESULTS`. An alias that
    /// is exactly the query finds its contact alone; otherwise the names and aliases
    /// that contain the query do.
    // shortcut: every name and alias is read per search (at most MAX_CONTACTS), FTS5 when searches get slow
    pub fn find_contacts(&self, query: &str) -> Result<Vec<Contact>, ChatError> {
        let needle = normalize_text(query);
        let connection = self.lock();
        let ids = match contact_of_alias(&connection, &needle)? {
            Some(contact_id) => vec![contact_id],
            None => containing(&connection, &needle)?,
        };
        ids.iter()
            .take(CONTACT_FIND_MAX_RESULTS)
            .map(|contact_id| contact_by_id(&connection, contact_id))
            .collect()
    }

    /// The contacts newest first, at most `limit`. With a `query`, only the contacts whose
    /// name or one alias contains it; with an `after`, only the contacts older than the one
    /// it names.
    // shortcut: every row is read per list (at most MAX_CONTACTS), FTS5 when lists get slow
    pub fn list_contacts(
        &self,
        query: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Contact>, ChatError> {
        let cursor = after
            .map(|contact_id| self.contact(contact_id))
            .transpose()?;
        let needle = query.map(normalize_text);
        let connection = self.lock();
        let mut contacts = Vec::new();
        for (contact_id, name, updated_at) in contact_rows(&connection)? {
            if contacts.len() == limit {
                break;
            }
            if let Some(cursor) = &cursor {
                if !older_than(&updated_at, &contact_id, (&cursor.updated_at, &cursor.id)) {
                    continue;
                }
            }
            if let Some(needle) = &needle {
                if !holds(&connection, &contact_id, &name, needle)? {
                    continue;
                }
            }
            contacts.push(contact_by_id(&connection, &contact_id)?);
        }
        Ok(contacts)
    }
}

fn contact_of_alias(connection: &Connection, alias: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT contact_id FROM contact_alias WHERE alias = ?1",
            [alias],
            |row| row.get(0),
        )
        .optional()?)
}

/// The ids of the contacts whose normalized name or one alias contains `needle`,
/// newest first.
fn containing(connection: &Connection, needle: &str) -> Result<Vec<String>, ChatError> {
    let mut ids = Vec::new();
    for (contact_id, name, _) in contact_rows(connection)? {
        if holds(connection, &contact_id, &name, needle)? {
            ids.push(contact_id);
        }
    }
    Ok(ids)
}

/// Whether the normalized name or one alias of the contact contains `needle`.
fn holds(
    connection: &Connection,
    contact_id: &str,
    name: &str,
    needle: &str,
) -> Result<bool, ChatError> {
    if normalize_text(name).contains(needle) {
        return Ok(true);
    }
    Ok(aliases_of(connection, contact_id)?
        .iter()
        .any(|alias| alias.contains(needle)))
}
