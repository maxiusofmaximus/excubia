//! The notes of Alfred and their audit trail (F2, version 14). A note keeps its
//! text and the message that made it; the audit keeps only metadata, never a
//! subject or a body, and it is append-only: the triggers refuse any change.

pub const AGENDA_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS note (
    id              TEXT PRIMARY KEY,
    subject         TEXT,
    body            TEXT NOT NULL,
    origin_kind     TEXT NOT NULL,
    origin_id       TEXT NOT NULL,
    conversation_id TEXT REFERENCES conversation(id) ON DELETE SET NULL,
    message_id      TEXT REFERENCES message(id) ON DELETE SET NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    updated_by      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agenda_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    object_kind TEXT NOT NULL,
    object_id   TEXT NOT NULL,
    action      TEXT NOT NULL,
    actor_kind  TEXT NOT NULL,
    actor_id    TEXT NOT NULL,
    at          TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS agenda_audit_no_update BEFORE UPDATE ON agenda_audit
BEGIN
    SELECT RAISE(ABORT, 'agenda_audit is append-only');
END;
CREATE TRIGGER IF NOT EXISTS agenda_audit_no_delete BEFORE DELETE ON agenda_audit
BEGIN
    SELECT RAISE(ABORT, 'agenda_audit is append-only');
END;
"#;

#[cfg(test)]
mod tests {
    use crate::schema::{open, SCHEMA_VERSION};
    use excubia_test_dir::TestDir;
    use rusqlite::Connection;

    fn has_table(connection: &Connection, name: &str) -> bool {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .is_ok_and(|count| count == 1)
    }

    /// A version 13 database has the chat rows and no notes. Version 14 keeps
    /// every row, adds the two tables and the audit can only grow.
    #[test]
    fn a_version_thirteen_database_gains_the_notes_and_keeps_its_rows() {
        let dir = TestDir::new("schema-v13-notes");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "DROP TABLE agenda_audit; DROP TABLE note; PRAGMA user_version = 13;",
                )
                .expect("back to version 13");
        }
        let companies_before = {
            let (connection, _) = open(&path).expect("reopen to version 14");
            let version: i64 = connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .expect("version");
            assert_eq!(version, SCHEMA_VERSION);
            assert!(has_table(&connection, "note"));
            assert!(has_table(&connection, "agenda_audit"));
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM company", [], |row| row.get(0))
                .expect("companies");
            count
        };
        assert_eq!(companies_before, 1);

        let (connection, _) = open(&path).expect("third open is a no-op");
        connection
            .execute(
                "INSERT INTO agenda_audit (object_kind, object_id, action, actor_kind, actor_id, at)
                 VALUES ('note', 'nt_1', 'create', 'alfred', 'ag_1', '2026-10-28T10:00:00Z')",
                [],
            )
            .expect("audit row");
        assert!(connection
            .execute("UPDATE agenda_audit SET action = 'x'", [])
            .is_err());
        assert!(connection.execute("DELETE FROM agenda_audit", []).is_err());
    }
}
