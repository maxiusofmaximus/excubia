//! The rules of the delegation tree and the helpers that read it: depth,
//! cycles, budgets, the card of a delegation and the bounded summaries.

use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};

use crate::error::ChatError;
use crate::model::{CardLinks, Message};
use crate::model_team::{Card, Delegation, DelegationState, NewDelegation, Seat};
use crate::schema;

use super::delegations::{CYCLE_WINDOW_MINUTES, DELEGATION_SELECT, MAX_DEPTH};
use super::next_seq;

pub(super) fn check_tree(
    connection: &Connection,
    from: &Seat,
    to: &Seat,
    new: &NewDelegation,
) -> Result<(i64, Option<String>), ChatError> {
    let conversation_project: Option<String> = connection
        .query_row(
            "SELECT project_id FROM conversation WHERE id = ?1",
            [&new.conversation_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let (depth, project_id) = match from.role_id.as_str() {
        "alfred" => {
            if to.role_id != "lead" || to.project_id != conversation_project {
                return Err(ChatError::ScopeDenied);
            }
            (1, to.project_id.clone())
        }
        "lead" => {
            let Some(parent_id) = new.parent_id.as_deref() else {
                return Err(ChatError::ScopeDenied);
            };
            let parent = delegation_by_id(connection, parent_id)?;
            if parent.to_agent_id != from.agent_id || !parent.state.is_active() {
                return Err(ChatError::ScopeDenied);
            }
            if parent.conversation_id != new.conversation_id
                || from.project_id != to.project_id
                || to.project_id != parent.project_id
            {
                return Err(ChatError::ScopeDenied);
            }
            if to.role_id != "specialist" {
                return Err(ChatError::ScopeDenied);
            }
            (parent.depth + 1, to.project_id.clone())
        }
        // A specialist is at depth 2: delegating again would be depth 3.
        _ => return Err(ChatError::DepthExceeded),
    };
    if depth > MAX_DEPTH {
        return Err(ChatError::DepthExceeded);
    }
    if let Some(parent_id) = new.parent_id.as_deref() {
        if in_ancestry(connection, parent_id, &to.agent_id)? {
            return Err(ChatError::Cycle);
        }
    }
    Ok((depth, project_id))
}

fn in_ancestry(connection: &Connection, parent_id: &str, target: &str) -> Result<bool, ChatError> {
    let mut cursor = Some(parent_id.to_string());
    let mut guard = 0;
    while let Some(id) = cursor {
        guard += 1;
        if guard > MAX_DEPTH + 2 {
            return Err(ChatError::Inconsistent("delegation tree is too deep"));
        }
        let row: Option<(String, Option<String>)> = connection
            .query_row(
                "SELECT from_agent_id, parent_id FROM delegation WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((from_agent_id, parent)) = row else {
            break;
        };
        if from_agent_id == target {
            return Ok(true);
        }
        cursor = parent;
    }
    Ok(false)
}

pub(super) fn recent_cycle(
    connection: &Connection,
    from: &str,
    to: &str,
    digest: &str,
) -> Result<bool, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM delegation
          WHERE from_agent_id = ?1 AND to_agent_id = ?2 AND digest = ?3
            AND created_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?4)",
        params![from, to, digest, format!("-{CYCLE_WINDOW_MINUTES} minutes")],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(super) fn task_digest(task: &str) -> String {
    let canonical = serde_json::json!({ "task": task }).to_string();
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(digest))
}

pub(super) fn assignment_title(from: &str, to: &str) -> String {
    if from.eq_ignore_ascii_case("Alfred") {
        format!("Alfred pidió a {to}")
    } else {
        format!("{from} asignó a {to}")
    }
}

pub(super) fn card_id(delegation_id: &str) -> String {
    format!("act_{delegation_id}")
}

/// The card of a delegation, as it is born.
pub(super) struct NewCard<'a> {
    pub(super) conversation_id: &'a str,
    pub(super) delegation_id: &'a str,
    pub(super) title: &'a str,
    pub(super) summary: &'a str,
    pub(super) state: DelegationState,
    /// The seat the task is addressed to: the card points at its panel.
    pub(super) seat: &'a Seat,
    pub(super) now: &'a str,
}

pub(super) fn insert_card(connection: &Connection, card: NewCard<'_>) -> Result<(), ChatError> {
    let seq = next_seq(connection, card.conversation_id)?;
    connection.execute(
        "INSERT INTO timeline_card
            (id, conversation_id, delegation_id, seq, title, summary, status,
             agent_id, pane_id, attention, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?10)",
        params![
            card_id(card.delegation_id),
            card.conversation_id,
            card.delegation_id,
            seq,
            card.title,
            card.summary,
            card.state.as_str(),
            card.seat.agent_id,
            card.seat.pane_id,
            card.now
        ],
    )?;
    Ok(())
}

pub(super) fn update_card(
    connection: &Connection,
    delegation_id: &str,
    title: &str,
    summary: &str,
    state: DelegationState,
    attention: i64,
    now: &str,
) -> Result<(), ChatError> {
    connection.execute(
        "UPDATE timeline_card
            SET title = ?1, summary = ?2, status = ?3, attention = ?4, updated_at = ?5
          WHERE delegation_id = ?6",
        params![
            title,
            summary,
            state.as_str(),
            attention,
            now,
            delegation_id
        ],
    )?;
    Ok(())
}

pub(super) fn close_run(
    connection: &Connection,
    delegation_id: &str,
    state: &str,
) -> Result<(), ChatError> {
    connection.execute(
        "UPDATE run SET state = ?1, finished_at = ?2
          WHERE id = (SELECT run_id FROM delegation WHERE id = ?3)
            AND state IN ('queued', 'running')",
        params![state, schema::now(connection)?, delegation_id],
    )?;
    Ok(())
}

pub(super) fn active_delegations(connection: &Connection) -> Result<u64, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM delegation WHERE state IN ('queued', 'running', 'waiting')",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

pub fn active_for_agent(
    connection: &Connection,
    agent_id: &str,
) -> Result<Option<Delegation>, ChatError> {
    let id: Option<String> = connection
        .query_row(
            "SELECT id FROM delegation
              WHERE to_agent_id = ?1 AND state IN ('queued', 'running', 'waiting')
              ORDER BY created_at DESC LIMIT 1",
            [agent_id],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => Ok(Some(delegation_by_id(connection, &id)?)),
        None => Ok(None),
    }
}

pub(super) fn active_for_pane(
    connection: &Connection,
    pane_id: &str,
) -> Result<Option<Delegation>, ChatError> {
    let id: Option<String> = connection
        .query_row(
            "SELECT id FROM delegation WHERE pane_id = ?1
              AND state IN ('queued', 'running', 'waiting') LIMIT 1",
            [pane_id],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => Ok(Some(delegation_by_id(connection, &id)?)),
        None => Ok(None),
    }
}

pub(super) fn card_for(
    connection: &Connection,
    delegation_id: &str,
) -> Result<Option<Card>, ChatError> {
    let card = connection
        .query_row(
            "SELECT id, conversation_id, delegation_id, seq, title, summary, status,
                    agent_id, pane_id, attention, created_at, updated_at
               FROM timeline_card WHERE delegation_id = ?1",
            [delegation_id],
            |row| {
                let pane_id: Option<String> = row.get(8)?;
                Ok(Card {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    delegation_id: row.get(2)?,
                    seq: row.get(3)?,
                    title: row.get(4)?,
                    summary: row.get(5)?,
                    status: row.get(6)?,
                    agent_id: row.get(7)?,
                    pane_id: pane_id.clone(),
                    attention: row.get(9)?,
                    links: CardLinks {
                        agent: pane_id.map(|pane| format!("/v1/agents/{pane}")),
                        delegation: format!("/v1/chat/delegations/{delegation_id}"),
                    },
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        )
        .optional()?;
    Ok(card)
}

pub(super) fn delegation_by_id(connection: &Connection, id: &str) -> Result<Delegation, ChatError> {
    connection
        .query_row(
            &format!("{DELEGATION_SELECT} WHERE d.id = ?1"),
            [id],
            delegation_row,
        )
        .optional()?
        .ok_or_else(|| ChatError::NoSuchDelegation(id.to_string()))
}

pub(super) fn delegation_row(row: &Row<'_>) -> rusqlite::Result<Delegation> {
    let state: String = row.get(9)?;
    Ok(Delegation {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        project_id: row.get(2)?,
        parent_id: row.get(3)?,
        from_agent_id: row.get(4)?,
        from_name: row.get(5)?,
        to_agent_id: row.get(6)?,
        to_name: row.get(7)?,
        task: row.get(8)?,
        state: DelegationState::from_db(&state).unwrap_or(DelegationState::Interrupted),
        depth: row.get(10)?,
        turn_key: row.get(11)?,
        pane_id: row.get(12)?,
        result_message_id: row.get(13)?,
        reason: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        finished_at: row.get(17)?,
    })
}

pub(super) fn bounded(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    let mut chars = line.chars();
    let mut value: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        value.pop();
        value.push('…');
    }
    value
}

pub(super) fn message_by_id(
    connection: &Connection,
    message_id: &str,
) -> Result<Option<Message>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, conversation_id, seq, author_kind, author_agent_id, body_md, state,
                    created_at
               FROM message WHERE id = ?1",
            [message_id],
            |row| {
                Ok(Message {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    seq: row.get(2)?,
                    author_kind: row.get(3)?,
                    author_agent_id: row.get(4)?,
                    body_md: row.get(5)?,
                    state: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()?)
}
