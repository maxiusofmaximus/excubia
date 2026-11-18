//! Alfred's coalesced synthesis: one turn per batch of finished delegations.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model::{RunState, Turn};
use crate::model_team::SynthesisRow;
use crate::schema;

/// The approvals the operator denied for this delegation or for one it delegated.
const DENIED_BY_OPERATOR: &str = "FROM approval
    WHERE status = 'denied' AND decider_kind = 'user'
      AND (delegation_id = ?1 OR delegation_id IN (
              SELECT id FROM delegation WHERE parent_id = ?1))";

/// Whether the operator denied an approval of this delegation (see `DENIED_BY_OPERATOR`).
pub(super) fn denied_by_operator(
    connection: &Connection,
    delegation_id: &str,
) -> Result<bool, ChatError> {
    let denied: i64 = connection.query_row(
        &format!("SELECT COUNT(*) {DENIED_BY_OPERATOR}"),
        [delegation_id],
        |row| row.get(0),
    )?;
    Ok(denied > 0)
}

/// The words of the latest denial that has any (see `DENIED_BY_OPERATOR`).
pub(super) fn operator_denial_reason(
    connection: &Connection,
    delegation_id: &str,
) -> Result<Option<String>, ChatError> {
    let reason = connection
        .query_row(
            &format!(
                "SELECT operator_reason {DENIED_BY_OPERATOR} AND operator_reason IS NOT NULL
                 ORDER BY decided_at DESC LIMIT 1"
            ),
            [delegation_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(reason)
}

impl super::ChatStore {
    pub fn pending_synthesis(&self, limit: usize) -> Result<Vec<SynthesisRow>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT delegation_id, conversation_id, result_message_id
               FROM delegation_synthesis WHERE state = 'pending'
              ORDER BY created_at ASC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok(SynthesisRow {
                delegation_id: row.get(0)?,
                conversation_id: row.get(1)?,
                result_message_id: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// One Alfred turn for a batch of delegations of the same conversation.
    pub fn enqueue_synthesis(
        &self,
        rows: &[SynthesisRow],
        alfred_id: &str,
        text: &str,
    ) -> Result<Turn, ChatError> {
        let Some(first) = rows.first() else {
            return Err(ChatError::Inconsistent("empty synthesis batch"));
        };
        if rows
            .iter()
            .any(|row| row.conversation_id != first.conversation_id)
        {
            return Err(ChatError::Inconsistent(
                "synthesis batch spans conversations",
            ));
        }
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        let run_id = id::new_id("run");
        transaction.execute(
            "INSERT INTO run (id, agent_id, pane_id, message_id, state, started_at)
             VALUES (?1, ?2, NULL, ?3, 'queued', ?4)",
            params![run_id, alfred_id, first.result_message_id, now],
        )?;
        for row in rows {
            transaction.execute(
                "UPDATE delegation_synthesis
                    SET state = 'queued', run_id = ?1, updated_at = ?2
                  WHERE delegation_id = ?3 AND state = 'pending'",
                params![run_id, now, row.delegation_id],
            )?;
        }
        transaction.commit()?;
        Ok(Turn {
            id: run_id,
            state: RunState::Queued,
            agent_id: alfred_id.to_string(),
            conversation_id: first.conversation_id.clone(),
            message_id: first.result_message_id.clone(),
            text: text.to_string(),
        })
    }

    /// Whether the operator denied an approval of this delegation or of one it delegated.
    pub fn delegation_denied_by_operator(&self, delegation_id: &str) -> Result<bool, ChatError> {
        denied_by_operator(&self.lock(), delegation_id)
    }

    /// The words the operator wrote with his latest denial in this delegation, if he wrote any.
    pub fn delegation_denial_reason(
        &self,
        delegation_id: &str,
    ) -> Result<Option<String>, ChatError> {
        operator_denial_reason(&self.lock(), delegation_id)
    }

    /// The host wrote the close of these rows itself: they are done, no turn is left.
    pub fn mark_synthesis_done(&self, rows: &[SynthesisRow]) -> Result<(), ChatError> {
        let connection = self.lock();
        for row in rows {
            connection.execute(
                "UPDATE delegation_synthesis SET state = 'done', updated_at = ?1
                  WHERE delegation_id = ?2 AND state = 'queued'",
                params![schema::now(&connection)?, row.delegation_id],
            )?;
        }
        Ok(())
    }

    pub fn skip_synthesis(&self, rows: &[SynthesisRow], reason: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        for row in rows {
            connection.execute(
                "UPDATE delegation_synthesis SET state = 'skipped', updated_at = ?1
                  WHERE delegation_id = ?2 AND state IN ('pending', 'queued')",
                params![schema::now(&connection)?, row.delegation_id],
            )?;
        }
        eprintln!(
            "chat: synthesis skipped for {} delegation(s): {reason}",
            rows.len()
        );
        Ok(())
    }
}
