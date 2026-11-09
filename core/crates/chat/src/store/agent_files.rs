//! The files Alfred creates (DOC8). Each one is a ready attachment of its conversation
//! that the run which made it keeps and no message takes yet. A reply names the ones
//! it shows; a turn that ends with the others delivers them in short messages of Alfred.

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use super::append_event;
use super::attachments::{check_quota, link_in, read, require_open_conversation};
use crate::error::{AttachmentError, ChatError};
use crate::id;
use crate::model::{preview, ChatFrame};
use crate::model_attachment::{check_name, stored_size, Attachment, MAX_ATTACHMENTS_PER_MESSAGE};
use crate::schema;

impl super::ChatStore {
    /// Store a file Alfred made in `conversation_id` during `run_id`. The bytes are
    /// written before the row commits and removed again when the commit fails.
    pub fn create_agent_file(
        &self,
        conversation_id: &str,
        run_id: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<Attachment, AttachmentError> {
        check_name(name)?;
        let size = bytes.len() as u64;
        let stored = stored_size(size)?;
        let sha256 = hex::encode(Sha256::digest(bytes));
        let files = self.attachment_files().ok_or_else(|| {
            AttachmentError::Io(std::io::Error::other("attachments root is not set"))
        })?;
        let id = id::new_id("att");
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        require_open_conversation(&transaction, conversation_id)?;
        check_quota(&transaction, conversation_id, None, size)?;
        let now = schema::now(&transaction)?;
        transaction.execute(
            "INSERT INTO attachment
                (id, conversation_id, client_id, name, declared_size, declared_sha256, state,
                 mime, width, height, final_size, final_sha256, created_at, created_run)
             VALUES (?1, ?2, ?1, ?3, ?4, ?5, 'ready', ?6, 0, 0, ?4, ?5, ?7, ?8)",
            params![id, conversation_id, name, stored, sha256, mime, now, run_id],
        )?;
        let attachment = read(&transaction, &id)?.ok_or(AttachmentError::NoSuchAttachment)?;
        files.finish(conversation_id, &id, bytes)?;
        if let Err(error) = transaction.commit() {
            // The commit's error is the one to report; a file left behind is only clutter.
            let _ = files.remove(conversation_id, &id);
            return Err(error.into());
        }
        Ok(attachment)
    }
}

/// A reply may name only files Alfred made in this conversation. The rest of the
/// checks (not in a message yet, ready) are `link_in`'s.
pub(super) fn check_agent_files(
    connection: &Connection,
    conversation_id: &str,
    attachment_ids: &[&str],
) -> Result<(), AttachmentError> {
    for &attachment_id in attachment_ids {
        let made: Option<bool> = connection
            .query_row(
                "SELECT created_run IS NOT NULL FROM attachment
                  WHERE id = ?1 AND conversation_id = ?2",
                params![attachment_id, conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        if made != Some(true) {
            return Err(AttachmentError::UnknownAttachment);
        }
    }
    Ok(())
}

/// The journal lines of an agent message just written: `created`, then `updated`
/// as complete.
pub(super) fn announce_message(
    connection: &Connection,
    conversation_id: &str,
    message_id: &str,
    seq: i64,
    text: &str,
) -> Result<Vec<ChatFrame>, ChatError> {
    let created = append_event(
        connection,
        "chat.message",
        conversation_id,
        serde_json::json!({
            "event": "chat.message",
            "conversation_id": conversation_id,
            "message_id": message_id,
            "seq": seq,
            "change": "created",
            "author_kind": "agent",
            "preview": preview(text),
        }),
    )?;
    let updated = append_event(
        connection,
        "chat.message",
        conversation_id,
        serde_json::json!({
            "event": "chat.message",
            "conversation_id": conversation_id,
            "message_id": message_id,
            "seq": seq,
            "change": "updated",
            "author_kind": "agent",
            "state": "complete",
            "preview": preview(text),
        }),
    )?;
    Ok(vec![created, updated])
}

impl super::ChatStore {
    /// Deliver the files of `run_id` that its reply did not name: short messages of
    /// Alfred, four files at most each. Returns the journal lines they wrote.
    pub(super) fn deliver_unnamed_files(
        &self,
        connection: &Connection,
        conversation_id: &str,
        agent_id: &str,
        run_id: &str,
    ) -> Result<Vec<ChatFrame>, ChatError> {
        let unnamed = unnamed_files(connection, conversation_id, run_id)?;
        let mut frames = Vec::new();
        for chunk in unnamed.chunks(MAX_ATTACHMENTS_PER_MESSAGE) {
            let names: Vec<&str> = chunk.iter().map(|(_, name)| name.as_str()).collect();
            let ids: Vec<&str> = chunk.iter().map(|(id, _)| id.as_str()).collect();
            let text = delivery_text(&names);
            let (message_id, seq, _) = self.insert_message(
                connection,
                conversation_id,
                "agent",
                Some(agent_id),
                &text,
                "complete",
            )?;
            link_in(connection, conversation_id, &message_id, &ids)?;
            frames.extend(announce_message(
                connection,
                conversation_id,
                &message_id,
                seq,
                &text,
            )?);
        }
        Ok(frames)
    }
}

/// The files `run_id` made in the conversation that no message takes, oldest first.
fn unnamed_files(
    connection: &Connection,
    conversation_id: &str,
    run_id: &str,
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut statement = connection.prepare(
        "SELECT id, name FROM attachment
          WHERE conversation_id = ?1 AND created_run = ?2 AND message_id IS NULL
          ORDER BY created_at, id",
    )?;
    let rows = statement.query_map(params![conversation_id, run_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.collect()
}

fn delivery_text(names: &[&str]) -> String {
    match names {
        [name] => format!("Te dejo el archivo {name}"),
        _ => format!("Te dejo los archivos {}", names.join(", ")),
    }
}
