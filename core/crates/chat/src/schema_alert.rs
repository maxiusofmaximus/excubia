//! Version 21: the user's alert rules (F5). A rule names a trigger, an agent or none, a status
//! for a pane, and whether it notifies or mutes. The table is created with `IF NOT EXISTS`, so
//! the step can run again. The mode is not a column: it lives in `meta` under `alerts_mode`.

use rusqlite::Connection;

use crate::error::ChatError;

pub const ALERT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS alert_rule (
    id            TEXT PRIMARY KEY,
    trigger       TEXT NOT NULL CHECK (trigger IN ('pane_status', 'calendar_reminder')),
    agent         TEXT,
    status_filter TEXT CHECK (status_filter IN ('blocked', 'done')),
    action        TEXT NOT NULL CHECK (action IN ('notify', 'mute')),
    enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_by    TEXT NOT NULL CHECK (created_by IN ('user', 'alfred')),
    created_at    TEXT NOT NULL,
    CHECK (trigger = 'pane_status' OR (agent IS NULL AND status_filter IS NULL)),
    CHECK (trigger = 'calendar_reminder' OR status_filter IS NOT NULL)
);
"#;

/// The step to version 21, in its own transaction, as `migrate` runs each step.
pub fn migrate_to_21(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(ALERT_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 21)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use excubia_core_test_dir::TestDir;
    use rusqlite::Connection;

    use super::ALERT_SCHEMA;
    use crate::schema::{open, SCHEMA_VERSION};

    fn count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count")
    }

    /// A version 20 database keeps its rows; version 21 adds the table, and the step runs again.
    #[test]
    fn a_version_twenty_database_gains_the_rules_and_keeps_its_rows() {
        let dir = TestDir::new("schema-v20-alerts");
        let path = dir.join("chat.db");
        let companies = {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch("DROP TABLE alert_rule; PRAGMA user_version = 20;")
                .expect("back to version 20");
            count(&connection, "company")
        };
        assert!(companies > 0);

        let (connection, _) = open(&path).expect("reopen to version 21");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(count(&connection, "company"), companies);
        assert_eq!(count(&connection, "alert_rule"), 0);

        connection
            .execute_batch(ALERT_SCHEMA)
            .expect("the step runs again");
    }
}
