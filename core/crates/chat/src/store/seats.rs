//! The durable seat record. CORE6B keeps Alfred's status, adapter kind and
//! role-prompt hash here; the CLI process itself is never persisted.

use rusqlite::{params, OptionalExtension};

use crate::error::ChatError;

impl super::ChatStore {
    pub fn alfred_id(&self) -> Result<String, ChatError> {
        let connection = self.lock();
        super::alfred_id(&connection)
    }

    pub fn set_seat_status(&self, agent_id: &str, status: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE agent_seat SET status = ?1 WHERE id = ?2",
            params![status, agent_id],
        )?;
        Ok(())
    }

    pub fn set_seat_adapter(&self, agent_id: &str, kind: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE agent_seat SET kind = ?1 WHERE id = ?2",
            params![kind, agent_id],
        )?;
        Ok(())
    }

    pub fn set_seat_prompt_hash(&self, agent_id: &str, hash: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE agent_seat SET prompt_hash = ?1 WHERE id = ?2",
            params![hash, agent_id],
        )?;
        Ok(())
    }

    /// Whether the blocking UAC adapter of this seat was verified. A seat
    /// whose preflight failed is `unsafe` and is never launched.
    pub fn set_seat_uac_state(&self, agent_id: &str, state: &str) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE agent_seat SET uac_state = ?1 WHERE id = ?2",
            params![state, agent_id],
        )?;
        Ok(())
    }

    /// A pane died: the seat survives, but its panel does not.
    pub fn clear_seat_pane(&self, pane_id: &str) -> Result<Option<String>, ChatError> {
        let connection = self.lock();
        let agent_id: Option<String> = connection
            .query_row(
                "SELECT id FROM agent_seat WHERE pane_id = ?1",
                [pane_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(agent_id) = &agent_id {
            connection.execute(
                "UPDATE agent_seat SET pane_id = NULL, status = 'stopped' WHERE id = ?1",
                [agent_id],
            )?;
        }
        Ok(agent_id)
    }

    /// The durable seat behind a live panel.
    pub fn seat_for_pane(&self, pane_id: &str) -> Result<Option<String>, ChatError> {
        let connection = self.lock();
        Ok(connection
            .query_row(
                "SELECT id FROM agent_seat WHERE pane_id = ?1",
                [pane_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}
