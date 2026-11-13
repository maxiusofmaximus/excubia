//! Version 20: the `.ics` exchange (CALENDARIO-F4c). An imported event keeps the UID its file
//! gave it in `uid`; a host event has none. One UID is one event, while events without one are
//! not limited. The column is added once, so the step can run again.

use rusqlite::Connection;

use crate::error::ChatError;
use crate::schema_approval::add_column_once;

/// Adds the column and its partial unique index.
pub fn add_ics_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    add_column_once(connection, "calendar_event", "uid", "TEXT")?;
    connection.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS calendar_event_uid
             ON calendar_event (uid) WHERE uid IS NOT NULL;",
    )
}

/// The step to version 20, in its own transaction, as `migrate` runs each step.
pub fn migrate_to_20(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    add_ics_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", 20)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::schema::{open, SCHEMA_VERSION};
    use excubia_test_dir::TestDir;
    use rusqlite::{params, Connection};

    use super::add_ics_schema;

    fn has_column(connection: &Connection, table: &str, column: &str) -> bool {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("table info");
        let names: Vec<String> = statement
            .query_map([], |row| row.get(1))
            .expect("columns")
            .map(Result::unwrap)
            .collect();
        names.iter().any(|name| name == column)
    }

    fn insert(connection: &Connection, id: &str, uid: Option<&str>) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO calendar_event (id, title, start_utc, all_day, timezone, origin_kind,
                 origin_id, created_at, updated_at, updated_by, uid)
             VALUES (?1, 'Llamada', '2026-11-03T08:30:00.000Z', 0, 'Europe/Madrid', 'device',
                 'device-1', '2026-10-29T00:00:00.000Z', '2026-10-29T00:00:00.000Z', 'device', ?2)",
            params![id, uid],
        )
    }

    /// A version 19 database keeps its events; version 20 adds the column with no UID on them,
    /// and running the step again changes nothing.
    #[test]
    fn a_version_nineteen_database_gains_the_uid_and_keeps_its_events() {
        let dir = TestDir::new("schema-v19-ics");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            insert(&connection, "ev_1", None).expect("an event");
            connection
                .execute_batch(
                    "DROP INDEX calendar_event_uid;
                     ALTER TABLE calendar_event DROP COLUMN uid;
                     PRAGMA user_version = 19;",
                )
                .expect("back to version 19");
        }

        let (connection, _) = open(&path).expect("reopen to version 20");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        assert!(has_column(&connection, "calendar_event", "uid"));
        let kept: (String, Option<String>) = connection
            .query_row(
                "SELECT title, uid FROM calendar_event WHERE id = 'ev_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("the event survives");
        assert_eq!(kept, ("Llamada".to_string(), None));

        add_ics_schema(&connection).expect("the step runs again");
    }

    /// One UID is one event; events without a UID are not counted against each other.
    #[test]
    fn a_uid_is_kept_once_and_events_without_one_are_not_limited() {
        let dir = TestDir::new("schema-uid-unique");
        let (connection, _) = open(&dir.join("chat.db")).expect("current schema");
        insert(&connection, "ev_1", Some("a@x")).expect("the first");
        assert!(insert(&connection, "ev_2", Some("a@x")).is_err());
        insert(&connection, "ev_3", None).expect("no uid");
        insert(&connection, "ev_4", None).expect("no uid either");
    }
}
