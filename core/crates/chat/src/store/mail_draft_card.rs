//! The draft card of a conversation (MAILUX-HOST §A). A draft the host created or
//! changed for a turn shows here: its recipients, its subject and the start of its
//! body. One card per conversation and draft; a change updates every card of that
//! draft in place, in whichever conversation it sits, so no card goes stale.

use rusqlite::{params, Connection, Row};
use serde_json::json;

use crate::error::ChatError;
use crate::id;
use crate::model::{ChatFrame, MailDraftLine, TimelineEntry};
use crate::schema;

use super::{append_event, conversation_exists, next_seq};

/// What a draft card shows: the draft, its recipients and subject, and the start
/// of its body. The body itself is never stored here.
pub struct MailDraftCard<'a> {
    pub account: &'a str,
    pub draft_id: &'a str,
    pub to: &'a [String],
    pub subject: &'a str,
    pub preview: &'a str,
}

impl super::ChatStore {
    /// Shows the draft in `conversation_id` (a card is added there only when it
    /// has none yet), then brings every card of the draft up to date. `None` takes
    /// no conversation: a change made from the app only refreshes the cards that
    /// exist. `NoSuchConversation` when the conversation is unknown.
    pub fn put_mail_draft_card(
        &self,
        conversation_id: Option<&str>,
        card: MailDraftCard<'_>,
    ) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if let Some(conversation_id) = conversation_id {
            if !conversation_exists(&transaction, conversation_id)? {
                return Err(ChatError::NoSuchConversation);
            }
            insert_if_absent(&transaction, conversation_id, &card)?;
        }
        let frames = refresh_all(&transaction, &card)?;
        transaction.commit()?;
        for frame in frames {
            self.publish(frame);
        }
        Ok(())
    }
}

/// Adds the card to the conversation at the next `seq`, unless it has one.
fn insert_if_absent(
    connection: &Connection,
    conversation_id: &str,
    card: &MailDraftCard<'_>,
) -> Result<(), ChatError> {
    let present: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM timeline_mail_draft
                         WHERE conversation_id = ?1 AND draft_id = ?2)",
        params![conversation_id, card.draft_id],
        |row| row.get(0),
    )?;
    if present {
        return Ok(());
    }
    let seq = next_seq(connection, conversation_id)?;
    let created_at = schema::now(connection)?;
    connection.execute(
        "INSERT INTO timeline_mail_draft
            (id, conversation_id, draft_id, seq, account, to_json, subject, preview, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id::new_id("mdc"),
            conversation_id,
            card.draft_id,
            seq,
            card.account,
            serde_json::to_string(card.to)?,
            card.subject,
            card.preview,
            created_at,
        ],
    )?;
    Ok(())
}

/// Brings every card of the draft up to date and returns one event per card.
fn refresh_all(
    connection: &Connection,
    card: &MailDraftCard<'_>,
) -> Result<Vec<ChatFrame>, ChatError> {
    let cards: Vec<(String, String)> = {
        let mut statement = connection
            .prepare("SELECT id, conversation_id FROM timeline_mail_draft WHERE draft_id = ?1")?;
        let rows = statement
            .query_map([card.draft_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        rows
    };
    let to_json = serde_json::to_string(card.to)?;
    let mut frames = Vec::with_capacity(cards.len());
    for (card_id, conversation_id) in cards {
        connection.execute(
            "UPDATE timeline_mail_draft
                SET account = ?1, to_json = ?2, subject = ?3, preview = ?4
              WHERE id = ?5",
            params![card.account, to_json, card.subject, card.preview, card_id],
        )?;
        frames.push(append_event(
            connection,
            "chat.card",
            &conversation_id,
            json!({
                "event": "chat.card",
                "conversation_id": conversation_id,
                "card_id": card_id,
                "kind": "mail_draft",
                "status": "updated",
            }),
        )?);
    }
    Ok(frames)
}

/// The draft cards of a conversation, as timeline entries.
pub(super) fn load_mail_draft_cards(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT id, seq, account, draft_id, to_json, subject, preview, created_at
               FROM timeline_mail_draft WHERE conversation_id = ?1 AND seq < ?2
              ORDER BY seq DESC LIMIT ?3"
        }
        None => {
            "SELECT id, seq, account, draft_id, to_json, subject, preview, created_at
               FROM timeline_mail_draft WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows: Vec<DraftRow> = match before_seq {
        Some(before) => statement
            .query_map(params![conversation_id, before, limit as i64], draft_row)?
            .collect::<Result<_, _>>()?,
        None => statement
            .query_map(params![conversation_id, limit as i64], draft_row)?
            .collect::<Result<_, _>>()?,
    };
    rows.into_iter()
        .map(|row| {
            Ok(TimelineEntry::mail_draft(MailDraftLine {
                seq: row.seq,
                card_id: row.card_id,
                account: row.account,
                draft_id: row.draft_id,
                to: serde_json::from_str(&row.to_json)?,
                subject: row.subject,
                preview: row.preview,
                created_at: row.created_at,
            }))
        })
        .collect()
}

/// One stored draft card before its recipients are read back from JSON.
struct DraftRow {
    card_id: String,
    seq: i64,
    account: String,
    draft_id: String,
    to_json: String,
    subject: String,
    preview: String,
    created_at: String,
}

fn draft_row(row: &Row<'_>) -> rusqlite::Result<DraftRow> {
    Ok(DraftRow {
        card_id: row.get(0)?,
        seq: row.get(1)?,
        account: row.get(2)?,
        draft_id: row.get(3)?,
        to_json: row.get(4)?,
        subject: row.get(5)?,
        preview: row.get(6)?,
        created_at: row.get(7)?,
    })
}
