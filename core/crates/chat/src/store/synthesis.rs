//! Alfred's coalesced synthesis: one turn per batch of finished delegations.

use rusqlite::params;

use crate::error::ChatError;
use crate::id;
use crate::model::{RunState, Turn};
use crate::model_team::SynthesisRow;
use crate::schema;

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
