//! The sign-in card of a mail login in the timeline (MAIL1D §2). It appears when
//! the login starts and goes when the login ends, however it ends. It carries
//! the user code and the page to type it on, never a device code or a token.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::json;

use crate::error::ChatError;
use crate::id;
use crate::model::{MailCardLine, TimelineEntry};
use crate::schema;

use super::{append_event, conversation_exists, next_seq};

/// A sign-in card to show: its login, what the user types and how long the
/// login waits for it.
pub struct NewMailCard<'a> {
    pub conversation_id: &'a str,
    pub login_id: &'a str,
    pub account: &'a str,
    pub user_code: &'a str,
    pub verification_uri: &'a str,
    pub expires_in: u64,
}

impl super::ChatStore {
    /// Show the sign-in card in a conversation. `NoSuchConversation` when the
    /// conversation is unknown.
    pub fn open_mail_card(&self, new: NewMailCard<'_>) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if !conversation_exists(&transaction, new.conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        let seq = next_seq(&transaction, new.conversation_id)?;
        let card_id = id::new_id("mcd");
        let now = schema::now(&transaction)?;
        let expires_at: String = transaction.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            [format!("+{} seconds", new.expires_in)],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO timeline_mail_card
                (id, conversation_id, login_id, seq, account, user_code,
                 verification_uri, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                card_id,
                new.conversation_id,
                new.login_id,
                seq,
                new.account,
                new.user_code,
                new.verification_uri,
                now,
                expires_at
            ],
        )?;
        let frame = append_event(
            &transaction,
            "chat.card",
            new.conversation_id,
            json!({
                "event": "chat.card",
                "conversation_id": new.conversation_id,
                "card_id": card_id,
                "kind": "mail_login",
                "status": "open",
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        Ok(())
    }

    /// Take the card away when its login ends, whatever the outcome. A login
    /// that never showed a card has none to take away.
    pub fn close_mail_card(&self, login_id: &str) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let card: Option<(String, String)> = transaction
            .query_row(
                "SELECT id, conversation_id FROM timeline_mail_card WHERE login_id = ?1",
                [login_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((card_id, conversation_id)) = card else {
            return Ok(());
        };
        transaction.execute("DELETE FROM timeline_mail_card WHERE id = ?1", [&card_id])?;
        let frame = append_event(
            &transaction,
            "chat.card",
            &conversation_id,
            json!({
                "event": "chat.card",
                "conversation_id": conversation_id,
                "card_id": card_id,
                "kind": "mail_login",
                "status": "closed",
            }),
        )?;
        transaction.commit()?;
        self.publish(frame);
        Ok(())
    }

    /// Take every sign-in card away. A login never outlives its host process,
    /// so a restart ends the logins the cards were waiting on.
    pub fn close_all_mail_cards(&self) -> Result<(), ChatError> {
        let login_ids = {
            let connection = self.lock();
            let mut statement = connection.prepare("SELECT login_id FROM timeline_mail_card")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for login_id in login_ids {
            self.close_mail_card(&login_id)?;
        }
        Ok(())
    }
}

/// The open sign-in cards of a conversation, as timeline entries.
pub(super) fn load_mail_cards(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT id, seq, account, user_code, verification_uri, created_at, expires_at
               FROM timeline_mail_card WHERE conversation_id = ?1 AND seq < ?2
              ORDER BY seq DESC LIMIT ?3"
        }
        None => {
            "SELECT id, seq, account, user_code, verification_uri, created_at, expires_at
               FROM timeline_mail_card WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows = match before_seq {
        Some(before) => statement
            .query_map(
                params![conversation_id, before, limit as i64],
                mail_card_row,
            )?
            .collect::<Result<Vec<_>, _>>()?,
        None => statement
            .query_map(params![conversation_id, limit as i64], mail_card_row)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

fn mail_card_row(row: &Row<'_>) -> rusqlite::Result<TimelineEntry> {
    Ok(TimelineEntry::mail_card(MailCardLine {
        card_id: row.get(0)?,
        seq: row.get(1)?,
        account: row.get(2)?,
        user_code: row.get(3)?,
        verification_uri: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
    }))
}
