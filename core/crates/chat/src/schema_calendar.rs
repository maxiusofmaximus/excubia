//! The calendar (CALENDARIO-F4, version 18): the events, the company's zone and the cards
//! a reminder leaves in a conversation. The events keep UTC instants and the zone they
//! were written in; a reminder is due at `start_utc - reminder_minutes`, computed when it
//! is read, so no derived column can go stale.

use rusqlite::Connection;

use crate::error::ChatError;
use crate::schema_approval::add_column_once;

const CALENDAR_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS calendar_event (
    id               TEXT PRIMARY KEY,
    title            TEXT NOT NULL,
    start_utc        TEXT NOT NULL,
    end_utc          TEXT,
    all_day          INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
    timezone         TEXT NOT NULL,
    location         TEXT,
    notes            TEXT,
    reminder_minutes INTEGER CHECK (reminder_minutes BETWEEN 0 AND 10080),
    reminded_at      TEXT,
    origin_kind      TEXT NOT NULL,
    origin_id        TEXT NOT NULL,
    conversation_id  TEXT REFERENCES conversation(id) ON DELETE SET NULL,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    updated_by       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS calendar_event_start ON calendar_event(start_utc, id);
CREATE INDEX IF NOT EXISTS calendar_event_reminder ON calendar_event(start_utc)
    WHERE reminder_minutes IS NOT NULL AND reminded_at IS NULL;

CREATE TABLE IF NOT EXISTS timeline_calendar_card (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    event_id        TEXT NOT NULL REFERENCES calendar_event(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE(conversation_id, seq)
);
"#;

/// Version 18. Every statement can run again, so a database that ran it twice keeps its rows.
pub fn add_calendar_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(CALENDAR_SCHEMA)?;
    add_column_once(connection, "company", "timezone", "TEXT")
}

/// The step to version 18, in its own transaction, as `migrate` runs each step.
pub fn migrate_to_18(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    add_calendar_schema(&transaction)?;
    transaction.pragma_update(None, "user_version", 18)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::schema::{open, SCHEMA_VERSION};
    use excubia_test_dir::TestDir;
    use rusqlite::Connection;

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

    /// A version 17 database has no calendar. Version 18 adds it, keeps the companies,
    /// and running the migration again changes nothing.
    #[test]
    fn a_version_seventeen_database_gains_the_calendar_and_keeps_its_rows() {
        let dir = TestDir::new("schema-v17-calendar");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "DROP TABLE timeline_calendar_card; DROP TABLE calendar_event;
                     ALTER TABLE company DROP COLUMN timezone; PRAGMA user_version = 17;",
                )
                .expect("back to version 17");
        }
        let (connection, _) = open(&path).expect("reopen to version 18");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        assert!(has_column(&connection, "company", "timezone"));
        assert!(has_column(&connection, "calendar_event", "reminded_at"));
        let companies: i64 = connection
            .query_row("SELECT COUNT(*) FROM company", [], |row| row.get(0))
            .expect("companies");
        assert_eq!(companies, 1);

        super::add_calendar_schema(&connection).expect("the migration runs again");
        super::add_calendar_schema(&connection).expect("and again");
        assert!(has_column(&connection, "company", "timezone"));
    }
}
