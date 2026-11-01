//! Deleting a conversation for good. The rows go in one transaction; the
//! attachment folder goes after its commit, so a failed file leaves the database
//! as it is. Alfred's notes stay: they lose only their links to the conversation.

use rusqlite::{params, Connection};
use serde_json::json;

use super::conversations::conversation_row;
use super::journal::append_journal;
use crate::error::ChatError;
use crate::schema::{self, SearchMode};

impl super::ChatStore {
    /// The user's own delete. Refused with `ConversationBusy` while a turn, a
    /// delegation or a pending approval of the conversation is still open.
    pub fn delete_conversation(
        &self,
        conversation_id: &str,
        device_id: &str,
    ) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        conversation_row(&transaction, conversation_id)?.ok_or(ChatError::NoSuchConversation)?;
        if has_open_work(&transaction, conversation_id)? {
            return Err(ChatError::ConversationBusy);
        }
        let now = schema::now(&transaction)?;
        // The references between runs, delegations and messages have no cascade.
        // Deferred, they are checked at the commit: a dangling one rolls all back.
        transaction.pragma_update(None, "defer_foreign_keys", "ON")?;
        transaction.execute(
            "UPDATE approval SET conversation_id = NULL, delegation_id = NULL
              WHERE conversation_id = ?1
                 OR delegation_id IN (SELECT id FROM delegation WHERE conversation_id = ?1)",
            [conversation_id],
        )?;
        transaction.execute(
            "DELETE FROM delegation_synthesis WHERE conversation_id = ?1",
            [conversation_id],
        )?;
        transaction.execute(
            "DELETE FROM run WHERE message_id IN (SELECT id FROM message WHERE conversation_id = ?1)
                OR id IN (SELECT run_id FROM delegation WHERE conversation_id = ?1)",
            [conversation_id],
        )?;
        transaction.execute(
            "DELETE FROM delegation WHERE conversation_id = ?1",
            [conversation_id],
        )?;
        // Tables without a foreign key to the conversation.
        transaction.execute(
            "DELETE FROM chat_event WHERE conversation_id = ?1",
            [conversation_id],
        )?;
        transaction.execute(
            "DELETE FROM hook_token WHERE conversation_id = ?1",
            [conversation_id],
        )?;
        if self.search == SearchMode::Fts5 {
            transaction.execute(
                "DELETE FROM conversation_fts WHERE conversation_id = ?1",
                [conversation_id],
            )?;
        }
        transaction.execute("DELETE FROM conversation WHERE id = ?1", [conversation_id])?;
        transaction.execute(
            "INSERT INTO agenda_audit (object_kind, object_id, action, actor_kind, actor_id, at)
             VALUES ('conversation', ?1, 'delete', 'device', ?2, ?3)",
            params![conversation_id, device_id, now],
        )?;
        // No conversation column: the journal keeps no row that names the deleted one.
        let frame = append_journal(
            &transaction,
            "chat.conversation",
            None,
            json!({
                "event": "chat.conversation",
                "conversation_id": conversation_id,
                "change": "deleted",
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        self.remove_attachment_folder(conversation_id);
        Ok(())
    }

    /// A failure here is logged without any name or content; the rows are gone.
    fn remove_attachment_folder(&self, conversation_id: &str) {
        let Some(files) = self.attachment_files() else {
            return;
        };
        if let Err(error) = files.remove_conversation(conversation_id) {
            eprintln!("chat: attachment files of a deleted conversation not removed: {error}");
        }
    }
}

fn has_open_work(connection: &Connection, conversation_id: &str) -> Result<bool, ChatError> {
    let open: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM run r
                         WHERE r.state IN ('queued', 'running') AND (
                               r.message_id IN (SELECT id FROM message WHERE conversation_id = ?1)
                            OR r.id IN (SELECT run_id FROM delegation WHERE conversation_id = ?1)))
             OR EXISTS (SELECT 1 FROM delegation
                         WHERE conversation_id = ?1 AND state IN ('queued', 'running', 'waiting'))
             OR EXISTS (SELECT 1 FROM approval
                         WHERE conversation_id = ?1 AND status = 'pending')",
        [conversation_id],
        |row| row.get(0),
    )?;
    Ok(open)
}
