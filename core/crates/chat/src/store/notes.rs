//! Alfred's notes (F2) and the notes the app keeps (AGENDA-REST): saving, editing,
//! listing, searching and deleting them. A write and its `agenda_audit` line share one
//! transaction, and the audit keeps metadata only. Alfred's delete is never made here on
//! its own: it is the approval's burn, in `approval_decisions`, once the user grants it.
//! The app's delete is the user's own act and runs at once.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::agenda::{older_than, write_audit, Writer};
use super::delegation_rules::bounded;
use crate::error::ChatError;
use crate::id;
use crate::model_note::{Note, NoteHit, MAX_NOTES};
use crate::seat::SeatActor;

/// The longest excerpt a search result shows, in characters.
const EXCERPT_CHARS: usize = 200;

const NOTE_SELECT: &str =
    "SELECT id, subject, body, conversation_id, message_id, created_at, updated_at FROM note";

impl super::ChatStore {
    /// Save a note that Alfred was asked to keep. The message that made it is the turn's
    /// own, or else the last user message of the conversation.
    pub fn save_note(
        &self,
        actor: &SeatActor,
        subject: Option<String>,
        body: String,
    ) -> Result<Note, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let note = Note {
            id: id::new_id("nt"),
            subject,
            body,
            conversation_id: non_empty(&actor.conversation_id),
            message_id: origin_message(&transaction, actor)?,
            created_at: now.clone(),
            updated_at: now,
        };
        insert_note(&transaction, Writer::seat(actor), &note)?;
        transaction.commit()?;
        Ok(note)
    }

    /// Save a note the app wrote. A `message_id` it names must be a message that exists.
    pub fn save_note_by_device(
        &self,
        device_id: &str,
        subject: Option<String>,
        body: String,
        message_id: Option<String>,
    ) -> Result<Note, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if let Some(message_id) = &message_id {
            require_message(&transaction, message_id)?;
        }
        let note = Note {
            id: id::new_id("nt"),
            subject,
            body,
            conversation_id: None,
            message_id,
            created_at: now.clone(),
            updated_at: now,
        };
        insert_note(&transaction, Writer::device(device_id), &note)?;
        transaction.commit()?;
        Ok(note)
    }

    /// Change the fields that are given. An empty subject clears it.
    pub fn update_note(
        &self,
        actor: &SeatActor,
        note_id: &str,
        subject: Option<String>,
        body: Option<String>,
    ) -> Result<Note, ChatError> {
        self.change_note(Writer::seat(actor), note_id, subject, body)
    }

    /// Change a note the app keeps, with the same rules as [`Self::update_note`].
    pub fn update_note_by_device(
        &self,
        device_id: &str,
        note_id: &str,
        subject: Option<String>,
        body: Option<String>,
    ) -> Result<Note, ChatError> {
        self.change_note(Writer::device(device_id), note_id, subject, body)
    }

    /// Delete a note the app asks to delete. The user asked for it, so it runs at once.
    pub fn delete_note_by_device(&self, device_id: &str, note_id: &str) -> Result<(), ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        note_by_id(&transaction, note_id)?;
        delete_in(&transaction, note_id, Writer::device(device_id), &now)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn note(&self, note_id: &str) -> Result<Note, ChatError> {
        let connection = self.lock();
        note_by_id(&connection, note_id)
    }

    /// The notes newest first, at most `limit`. With a `query` only the notes whose subject
    /// or body contains it; with a `before` only the notes older than the one it names. The
    /// match folds Unicode case, which SQLite's LIKE does not, so it runs here.
    // shortcut: every note is read per list (at most MAX_NOTES), FTS5 when searches get slow
    pub fn list_notes(
        &self,
        query: Option<&str>,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Vec<NoteHit>, ChatError> {
        let cursor = before.map(|note_id| self.note(note_id)).transpose()?;
        let needle = query.map(str::to_lowercase);
        let connection = self.lock();
        let mut statement =
            connection.prepare(&format!("{NOTE_SELECT} ORDER BY updated_at DESC, id DESC"))?;
        let notes = statement.query_map([], note_row)?;
        let mut hits = Vec::new();
        for note in notes {
            if hits.len() == limit {
                break;
            }
            let note = note?;
            if cursor.as_ref().is_some_and(|cursor| {
                !older_than(&note.updated_at, &note.id, (&cursor.updated_at, &cursor.id))
            }) {
                continue;
            }
            if needle
                .as_deref()
                .is_none_or(|needle| contains(&note, needle))
            {
                hits.push(hit_of(note));
            }
        }
        Ok(hits)
    }

    /// The notes a search of Alfred's finds, newest first, at most `limit`.
    pub fn search_notes(&self, query: &str, limit: usize) -> Result<Vec<NoteHit>, ChatError> {
        self.list_notes(Some(query), None, limit)
    }

    fn change_note(
        &self,
        writer: Writer<'_>,
        note_id: &str,
        subject: Option<String>,
        body: Option<String>,
    ) -> Result<Note, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let mut note = note_by_id(&transaction, note_id)?;
        if let Some(subject) = subject {
            note.subject = Some(subject).filter(|subject| !subject.is_empty());
        }
        if let Some(body) = body {
            note.body = body;
        }
        note.updated_at = now.clone();
        transaction.execute(
            "UPDATE note SET subject = ?2, body = ?3, updated_at = ?4, updated_by = ?5
              WHERE id = ?1",
            params![note.id, note.subject, note.body, now, writer.kind],
        )?;
        write_audit(
            &transaction,
            "note",
            &note.id,
            "update",
            writer.kind,
            writer.id,
            &now,
        )?;
        transaction.commit()?;
        Ok(note)
    }
}

/// Inserts a new note and its audit line. The count and the insert share one transaction,
/// so two saves cannot both pass the limit.
fn insert_note(
    transaction: &Transaction<'_>,
    writer: Writer<'_>,
    note: &Note,
) -> Result<(), ChatError> {
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM note", [], |row| row.get(0))?;
    if count >= MAX_NOTES {
        return Err(ChatError::NoteLimit);
    }
    transaction.execute(
        "INSERT INTO note (id, subject, body, origin_kind, origin_id, conversation_id,
                           message_id, created_at, updated_at, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?4)",
        params![
            note.id,
            note.subject,
            note.body,
            writer.kind,
            writer.id,
            note.conversation_id,
            note.message_id,
            note.created_at
        ],
    )?;
    write_audit(
        transaction,
        "note",
        &note.id,
        "create",
        writer.kind,
        writer.id,
        &note.created_at,
    )
}

/// The message that made the note: the one the turn names, or else the last user
/// message of the conversation.
fn origin_message(connection: &Connection, actor: &SeatActor) -> Result<Option<String>, ChatError> {
    let from_run = connection
        .query_row(
            "SELECT message_id FROM run WHERE id = ?1",
            [&actor.run_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    if from_run.is_some() {
        return Ok(from_run);
    }
    Ok(connection
        .query_row(
            "SELECT id FROM message
              WHERE conversation_id = ?1 AND author_kind = 'user'
              ORDER BY seq DESC LIMIT 1",
            [&actor.conversation_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// A note keeps a link to the message that made it, so that message must exist.
fn require_message(connection: &Connection, message_id: &str) -> Result<(), ChatError> {
    let found = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM message WHERE id = ?1)",
        [message_id],
        |row| row.get::<_, bool>(0),
    )?;
    if found {
        Ok(())
    } else {
        Err(ChatError::NoSuchMessage(message_id.to_string()))
    }
}

/// Delete a note inside the transaction that burns the user's approval, or that the app's
/// delete opens. A note already gone is no failure here. The audit names the writer, never
/// the text.
pub(super) fn delete_in(
    transaction: &Transaction<'_>,
    note_id: &str,
    writer: Writer<'_>,
    at: &str,
) -> Result<(), ChatError> {
    if transaction.execute("DELETE FROM note WHERE id = ?1", [note_id])? == 1 {
        write_audit(
            transaction,
            "note",
            note_id,
            "delete",
            writer.kind,
            writer.id,
            at,
        )?;
    }
    Ok(())
}

fn note_by_id(connection: &Connection, note_id: &str) -> Result<Note, ChatError> {
    connection
        .query_row(&format!("{NOTE_SELECT} WHERE id = ?1"), [note_id], note_row)
        .optional()?
        .ok_or_else(|| ChatError::NoteNotFound(note_id.to_string()))
}

fn note_row(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        subject: row.get(1)?,
        body: row.get(2)?,
        conversation_id: row.get(3)?,
        message_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn contains(note: &Note, needle: &str) -> bool {
    note.body.to_lowercase().contains(needle)
        || note
            .subject
            .as_deref()
            .is_some_and(|subject| subject.to_lowercase().contains(needle))
}

fn hit_of(note: Note) -> NoteHit {
    let flat = note.body.split_whitespace().collect::<Vec<_>>().join(" ");
    NoteHit {
        id: note.id,
        subject: note.subject,
        excerpt: bounded(&flat, EXCERPT_CHARS),
        updated_at: note.updated_at,
    }
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
