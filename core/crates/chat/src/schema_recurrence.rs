//! Version 19: recurring events (CALENDARIO-F4b). A series keeps its rule in `recurrence`, the
//! last occurrence its reminder announced in `reminded_occurrence`, and a reminder card the
//! occurrence it is about in `occurrence_start`. Each column is added once, so the step can
//! run again over a database that already has it.

use rusqlite::Connection;

use crate::error::ChatError;
use crate::schema_approval::add_column_once;

/// Adds the three columns. A card without an `occurrence_start` shows the event's own start.
pub fn add_recurrence_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    add_column_once(connection, "calendar_event", "recurrence", "TEXT")?;
    add_column_once(connection, "calendar_event", "reminded_occurrence", "TEXT")?;
    add_column_once(
        connection,
        "timeline_calendar_card",
        "occurrence_start",
        "TEXT",
    )
}

/// The step to version 19, in its own transaction, as `migrate` runs each step.
pub fn migrate_to_19(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    add_recurrence_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", 19)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::schema::{open, SCHEMA_VERSION};
    use excubia_test_dir::TestDir;
    use rusqlite::Connection;

    use super::add_recurrence_schema;

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

    /// A version 18 database keeps its events and its cards; version 19 adds the three
    /// columns, leaves the events as they were, and running the step again changes nothing.
    #[test]
    fn a_version_eighteen_database_gains_the_recurrence_and_keeps_its_events() {
        let dir = TestDir::new("schema-v18-recurrence");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "ALTER TABLE timeline_calendar_card DROP COLUMN occurrence_start;
                     ALTER TABLE calendar_event DROP COLUMN reminded_occurrence;
                     ALTER TABLE calendar_event DROP COLUMN recurrence;
                     INSERT INTO calendar_event (id, title, start_utc, all_day, timezone,
                         origin_kind, origin_id, created_at, updated_at, updated_by)
                     VALUES ('ev_1', 'Llamada', '2026-11-03T08:30:00.000Z', 0, 'Europe/Madrid',
                         'device', 'device-1', '2026-10-29T00:00:00.000Z',
                         '2026-10-29T00:00:00.000Z', 'device');
                     PRAGMA user_version = 18;",
                )
                .expect("back to version 18");
        }

        let (connection, _) = open(&path).expect("reopen to version 19");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        assert!(has_column(&connection, "calendar_event", "recurrence"));
        assert!(has_column(
            &connection,
            "calendar_event",
            "reminded_occurrence"
        ));
        assert!(has_column(
            &connection,
            "timeline_calendar_card",
            "occurrence_start"
        ));
        let kept: (String, Option<String>) = connection
            .query_row(
                "SELECT title, recurrence FROM calendar_event WHERE id = 'ev_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("the event survives");
        assert_eq!(kept, ("Llamada".to_string(), None));

        add_recurrence_schema(&connection).expect("the step runs again");
        assert!(has_column(&connection, "calendar_event", "recurrence"));
    }
}
