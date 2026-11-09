//! The database side of the attachments: uploads begin, become ready, are linked
//! to a message, and the quotas bound them. The bytes live in `attachments/`.

use std::collections::HashMap;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use crate::error::{AttachmentError, ChatError};
use crate::id;
use crate::model_attachment::{
    check_name, check_sha256, linked_from_row, stored_size, Attachment, AttachmentState,
    LinkedFile, ReadyFile, COLUMNS, CONVERSATION_QUOTA_BYTES, HOST_QUOTA_BYTES,
    MAX_ATTACHMENTS_PER_MESSAGE,
};
use crate::schema;

impl super::ChatStore {
    /// Start an upload. `client_id` is the idempotency key: repeating it returns
    /// the attachment the first call made. The declared size counts against the
    /// quotas from now on.
    pub fn begin_attachment(
        &self,
        conversation_id: &str,
        client_id: &str,
        name: &str,
        size: u64,
        sha256: &str,
    ) -> Result<Attachment, AttachmentError> {
        check_name(name)?;
        let declared_sha256 = check_sha256(sha256)?;
        let stored = stored_size(size)?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        require_open_conversation(&transaction, conversation_id)?;
        if let Some(existing) = find_by_client(&transaction, conversation_id, client_id)? {
            return Ok(existing);
        }
        check_quota(&transaction, conversation_id, None, size)?;
        let id = id::new_id("att");
        let now = schema::now(&transaction)?;
        transaction.execute(
            "INSERT INTO attachment
                (id, conversation_id, client_id, name, declared_size, declared_sha256, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'uploading', ?7)",
            params![id, conversation_id, client_id, name, stored, declared_sha256, now],
        )?;
        let attachment = read(&transaction, &id)?.ok_or(AttachmentError::NoSuchAttachment)?;
        transaction.commit()?;
        Ok(attachment)
    }

    pub fn attachment(&self, attachment_id: &str) -> Result<Attachment, AttachmentError> {
        let connection = self.lock();
        read(&connection, attachment_id)?.ok_or(AttachmentError::NoSuchAttachment)
    }

    /// The upload is complete. The final content replaces the declared one, so
    /// the quota is checked again with the final size.
    pub fn mark_ready(&self, attachment_id: &str, file: &ReadyFile) -> Result<(), AttachmentError> {
        let stored = stored_size(file.size)?;
        let sha256 = check_sha256(&file.sha256)?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current =
            read(&transaction, attachment_id)?.ok_or(AttachmentError::NoSuchAttachment)?;
        if current.state != AttachmentState::Uploading {
            return Err(AttachmentError::NotUploading);
        }
        check_quota(
            &transaction,
            &current.conversation_id,
            Some(attachment_id),
            file.size,
        )?;
        transaction.execute(
            "UPDATE attachment
                SET state = 'ready', mime = ?2, width = ?3, height = ?4,
                    final_size = ?5, final_sha256 = ?6
              WHERE id = ?1",
            params![
                attachment_id,
                file.mime,
                i64::from(file.width),
                i64::from(file.height),
                stored,
                sha256
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Attach ready files to a message already written in this conversation. Either
    /// every id is linked or none is. Sending a message links through
    /// `append_user_message_with_attachments` instead, in the same transaction.
    pub fn link_to_message(
        &self,
        conversation_id: &str,
        message_id: &str,
        attachment_ids: &[&str],
    ) -> Result<(), AttachmentError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        require_message(&transaction, conversation_id, message_id)?;
        link_in(&transaction, conversation_id, message_id, attachment_ids)?;
        transaction.commit()?;
        Ok(())
    }

    /// Drop the attachments of these ids that no message takes. Returns the ids it
    /// dropped, so the caller removes their files as well.
    pub fn delete_unlinked(
        &self,
        conversation_id: &str,
        attachment_ids: &[&str],
    ) -> Result<Vec<String>, AttachmentError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let mut removed = Vec::new();
        for &attachment_id in attachment_ids {
            let rows = transaction.execute(
                "DELETE FROM attachment
                  WHERE id = ?1 AND conversation_id = ?2 AND message_id IS NULL",
                params![attachment_id, conversation_id],
            )?;
            if rows == 1 {
                removed.push(attachment_id.to_string());
            }
        }
        transaction.commit()?;
        Ok(removed)
    }
}

pub(super) fn require_open_conversation(
    connection: &Connection,
    conversation_id: &str,
) -> Result<(), AttachmentError> {
    let archived: Option<i64> = connection
        .query_row(
            "SELECT archived FROM conversation WHERE id = ?1",
            [conversation_id],
            |row| row.get(0),
        )
        .optional()?;
    match archived {
        None => Err(ChatError::NoSuchConversation.into()),
        Some(0) => Ok(()),
        Some(_) => Err(ChatError::ConversationArchived.into()),
    }
}

fn require_message(
    connection: &Connection,
    conversation_id: &str,
    message_id: &str,
) -> Result<(), AttachmentError> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM message WHERE id = ?1 AND conversation_id = ?2",
            params![message_id, conversation_id],
            |row| row.get(0),
        )
        .optional()?;
    found.map(|_| ()).ok_or(AttachmentError::UnknownMessage)
}

/// Link ready files to the message just written. It runs inside that message's
/// transaction, so a refusal takes the message back with it: every id is linked
/// or none is.
pub(crate) fn link_in(
    connection: &Connection,
    conversation_id: &str,
    message_id: &str,
    attachment_ids: &[&str],
) -> Result<(), AttachmentError> {
    if attachment_ids.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(AttachmentError::TooManyAttachments);
    }
    for &attachment_id in attachment_ids {
        let attachment = read(connection, attachment_id)?
            .filter(|attachment| attachment.conversation_id == conversation_id)
            .ok_or(AttachmentError::UnknownAttachment)?;
        if attachment.message_id.is_some() {
            return Err(AttachmentError::AlreadyLinked);
        }
        if attachment.state != AttachmentState::Ready {
            return Err(AttachmentError::NotReady);
        }
        connection.execute(
            "UPDATE attachment SET message_id = ?2 WHERE id = ?1",
            params![attachment_id, message_id],
        )?;
    }
    Ok(())
}

/// The files of these messages, grouped by message, from one query for the whole
/// page. Messages without files are absent from the map.
pub(crate) fn linked_files(
    connection: &Connection,
    message_ids: &[&str],
) -> rusqlite::Result<HashMap<String, Vec<LinkedFile>>> {
    let mut grouped: HashMap<String, Vec<LinkedFile>> = HashMap::new();
    if message_ids.is_empty() {
        return Ok(grouped);
    }
    let marks = vec!["?"; message_ids.len()].join(",");
    let sql = format!(
        "SELECT message_id, id, name, mime, final_size, width, height
           FROM attachment WHERE message_id IN ({marks})
          ORDER BY created_at, id"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(message_ids), linked_from_row)?;
    for row in rows {
        let (message_id, file) = row?;
        grouped.entry(message_id).or_default().push(file);
    }
    Ok(grouped)
}

pub(super) fn read(
    connection: &Connection,
    attachment_id: &str,
) -> Result<Option<Attachment>, AttachmentError> {
    let sql = format!("SELECT {COLUMNS} FROM attachment WHERE id = ?1");
    Ok(connection
        .query_row(&sql, [attachment_id], Attachment::from_row)
        .optional()?)
}

fn find_by_client(
    connection: &Connection,
    conversation_id: &str,
    client_id: &str,
) -> Result<Option<Attachment>, AttachmentError> {
    let sql =
        format!("SELECT {COLUMNS} FROM attachment WHERE conversation_id = ?1 AND client_id = ?2");
    Ok(connection
        .query_row(
            &sql,
            params![conversation_id, client_id],
            Attachment::from_row,
        )
        .optional()?)
}

/// Refuse `added` bytes when the conversation or the host would pass its quota.
/// `except` leaves one attachment out of the count: the one being completed,
/// whose declared size is about to be replaced by its final size.
pub(super) fn check_quota(
    connection: &Connection,
    conversation_id: &str,
    except: Option<&str>,
    added: u64,
) -> Result<(), AttachmentError> {
    let conversation = usage(connection, Some(conversation_id), except)?;
    if conversation.saturating_add(added) > CONVERSATION_QUOTA_BYTES {
        return Err(AttachmentError::ConversationQuota);
    }
    let host = usage(connection, None, except)?;
    if host.saturating_add(added) > HOST_QUOTA_BYTES {
        return Err(AttachmentError::HostQuota);
    }
    Ok(())
}

/// Bytes kept by the attachments of one conversation, or of the whole host.
fn usage(
    connection: &Connection,
    conversation_id: Option<&str>,
    except: Option<&str>,
) -> Result<u64, AttachmentError> {
    let used: i64 = connection.query_row(
        "SELECT COALESCE(SUM(CASE WHEN state = 'ready' THEN final_size ELSE declared_size END), 0)
           FROM attachment
          WHERE (?1 IS NULL OR conversation_id = ?1) AND (?2 IS NULL OR id != ?2)",
        params![conversation_id, except],
        |row| row.get(0),
    )?;
    u64::try_from(used).map_err(|_| ChatError::Inconsistent("attachment sizes").into())
}
