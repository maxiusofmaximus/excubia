//! Why a turn failed and which model wrote a reply (version 31). `run.model` is the model
//! the turn runs on, copied to the reply message when the reply commits; `failure_reason` is
//! the run's reason on a failed card, so the shell can say what went wrong.

use rusqlite::Connection;

use crate::error::ChatError;
use crate::schema_approval::add_column_once;

/// The step to version 31, in its own transaction, as `migrate` runs each step. Each column is
/// added only when missing: a test that resets the version keeps the columns it already has.
pub fn migrate_to_31(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    add_column_once(&transaction, "message", "failure_reason", "TEXT")?;
    add_column_once(&transaction, "message", "model", "TEXT")?;
    add_column_once(&transaction, "run", "model", "TEXT")?;
    transaction.pragma_update(None, "user_version", 31)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use excubia_test_dir::TestDir;
    use rusqlite::Connection;

    use crate::schema::{open, SCHEMA_VERSION};

    fn count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count")
    }

    /// A version 30 database keeps its rows; version 31 adds the three columns, and the step runs again.
    #[test]
    fn a_version_thirty_database_gains_the_reason_and_model_and_keeps_its_rows() {
        let dir = TestDir::new("schema-v30-message-meta");
        let path = dir.join("chat.db");
        let companies = {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "ALTER TABLE message DROP COLUMN failure_reason;
                     ALTER TABLE message DROP COLUMN model;
                     ALTER TABLE run DROP COLUMN model;
                     PRAGMA user_version = 30;",
                )
                .expect("back to version 30");
            count(&connection, "company")
        };
        assert!(companies > 0);

        let (connection, _) = open(&path).expect("reopen to version 31");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(count(&connection, "company"), companies);
        for sql in ["SELECT failure_reason, model FROM message", "SELECT model FROM run"] {
            connection.prepare(sql).expect("the columns exist");
        }
    }
}
