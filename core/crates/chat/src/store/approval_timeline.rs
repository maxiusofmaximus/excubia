//! The timeline, merged: the messages, the cards and the approval lines of a
//! conversation share one `seq` order, and this module reads them as one page.

use rusqlite::{params, Connection, Row};

use crate::error::ChatError;
use crate::model::{ApprovalLine, TimelineEntry, TimelinePage};
use crate::model_team::Card;

use super::conversation_exists;
use super::messages::load_desc;

impl super::ChatStore {
    pub fn timeline(
        &self,
        conversation_id: &str,
        before_seq: Option<i64>,
        limit: usize,
    ) -> Result<TimelinePage, ChatError> {
        self.expire_now()?;
        let connection = self.lock();
        if !conversation_exists(&connection, conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        // Messages and cards share the `seq` namespace. Reading one extra of
        // each and merging in memory keeps one pagination rule for both.
        let mut entries = load_desc(&connection, conversation_id, before_seq, limit + 1)?
            .iter()
            .map(TimelineEntry::message)
            .collect::<Vec<_>>();
        entries.extend(
            load_cards(&connection, conversation_id, before_seq, limit + 1)?
                .iter()
                .map(Card::timeline_entry),
        );
        entries.extend(load_lines(
            &connection,
            conversation_id,
            before_seq,
            limit + 1,
        )?);
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.seq));
        let has_more = entries.len() > limit;
        entries.truncate(limit);
        entries.reverse();
        let next_before_seq = if has_more {
            entries.first().map(|entry| entry.seq)
        } else {
            None
        };
        Ok(TimelinePage {
            entries,
            next_before_seq,
        })
    }
}

fn load_cards(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<Card>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT id, conversation_id, delegation_id, seq, title, summary, status,
                    agent_id, pane_id, attention, created_at, updated_at
               FROM timeline_card WHERE conversation_id = ?1 AND seq < ?2
              ORDER BY seq DESC LIMIT ?3"
        }
        None => {
            "SELECT id, conversation_id, delegation_id, seq, title, summary, status,
                    agent_id, pane_id, attention, created_at, updated_at
               FROM timeline_card WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows = match before_seq {
        Some(before) => statement
            .query_map(params![conversation_id, before, limit as i64], card_row)?
            .collect::<Result<Vec<_>, _>>()?,
        None => statement
            .query_map(params![conversation_id, limit as i64], card_row)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

fn card_row(row: &Row<'_>) -> rusqlite::Result<Card> {
    let pane_id: Option<String> = row.get(8)?;
    let delegation_id: String = row.get(2)?;
    Ok(Card {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        delegation_id: delegation_id.clone(),
        seq: row.get(3)?,
        title: row.get(4)?,
        summary: row.get(5)?,
        status: row.get(6)?,
        agent_id: row.get(7)?,
        pane_id: pane_id.clone(),
        attention: row.get(9)?,
        links: crate::model::CardLinks {
            agent: pane_id.map(|pane| format!("/v1/agents/{pane}")),
            delegation: format!("/v1/chat/delegations/{delegation_id}"),
        },
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub(super) fn load_lines(
    connection: &Connection,
    conversation_id: &str,
    before_seq: Option<i64>,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ChatError> {
    let sql = match before_seq {
        Some(_) => {
            "SELECT approval_id, seq, status, summary, decider_kind, decider_id, created_at
               FROM timeline_approval WHERE conversation_id = ?1 AND seq < ?2
              ORDER BY seq DESC LIMIT ?3"
        }
        None => {
            "SELECT approval_id, seq, status, summary, decider_kind, decider_id, created_at
               FROM timeline_approval WHERE conversation_id = ?1
              ORDER BY seq DESC LIMIT ?2"
        }
    };
    let mut statement = connection.prepare(sql)?;
    let rows = match before_seq {
        Some(before) => statement
            .query_map(
                params![conversation_id, before, limit as i64],
                approval_line_row,
            )?
            .collect::<Result<Vec<_>, _>>()?,
        None => statement
            .query_map(params![conversation_id, limit as i64], approval_line_row)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

fn approval_line_row(row: &Row<'_>) -> rusqlite::Result<TimelineEntry> {
    Ok(TimelineEntry::approval(ApprovalLine {
        approval_id: row.get(0)?,
        seq: row.get(1)?,
        status: row.get(2)?,
        summary: row.get(3)?,
        decider_kind: row.get(4)?,
        decider_id: row.get(5)?,
        created_at: row.get(6)?,
    }))
}
