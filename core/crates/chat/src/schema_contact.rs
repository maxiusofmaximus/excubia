//! The address book (F3, version 15). An address belongs to one contact across the
//! whole company, and an alias names one contact: both are unique, so a lookup never
//! finds two. Deleting a contact deletes its addresses and aliases with it.

use rusqlite::Connection;

use crate::schema_approval::add_column_once;

pub const CONTACT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS contact (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    notes       TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    updated_by  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contact_address (
    contact_id  TEXT NOT NULL REFERENCES contact(id) ON DELETE CASCADE,
    address     TEXT NOT NULL,
    label       TEXT,
    UNIQUE (address)
);

CREATE TABLE IF NOT EXISTS contact_alias (
    contact_id  TEXT NOT NULL REFERENCES contact(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    UNIQUE (alias)
);
"#;

/// Version 16: the origin of each address. Every address kept so far was Alfred's;
/// `phone` is admitted now so the phone-book import needs no further migration. The
/// column is added only when it is missing, so the migration can run again.
pub fn add_source_column(connection: &Connection) -> Result<(), rusqlite::Error> {
    add_column_once(
        connection,
        "contact_address",
        "source",
        "TEXT NOT NULL DEFAULT 'alfred' CHECK (source IN ('alfred', 'user_message', 'phone'))",
    )
}

/// Version 17: `app` joins the origins, for the addresses the app writes. SQLite cannot
/// change a CHECK in place, so the table is rebuilt with every row. A table that already
/// admits `app` is left alone, so the migration can run again.
pub fn admit_app_source(connection: &Connection) -> Result<(), rusqlite::Error> {
    let table_sql: String = connection.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'contact_address'",
        [],
        |row| row.get(0),
    )?;
    if table_sql.contains("'app'") {
        return Ok(());
    }
    connection.execute_batch(
        "CREATE TABLE contact_address_v17 (
             contact_id  TEXT NOT NULL REFERENCES contact(id) ON DELETE CASCADE,
             address     TEXT NOT NULL,
             label       TEXT,
             source      TEXT NOT NULL DEFAULT 'alfred'
                         CHECK (source IN ('alfred', 'user_message', 'phone', 'app')),
             UNIQUE (address)
         );
         INSERT INTO contact_address_v17 (contact_id, address, label, source)
             SELECT contact_id, address, label, source FROM contact_address;
         DROP TABLE contact_address;
         ALTER TABLE contact_address_v17 RENAME TO contact_address;",
    )
}

#[cfg(test)]
mod tests {
    use super::admit_app_source;
    use crate::schema::{open, open_in_memory, SCHEMA_VERSION};
    use excubia_test_dir::TestDir;
    use rusqlite::params;

    /// A version 14 database has the notes and no contacts. Version 15 keeps every
    /// row and adds the three tables.
    #[test]
    fn a_version_fourteen_database_gains_the_contacts_and_keeps_its_notes() {
        let dir = TestDir::new("schema-v14-contacts");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "DROP TABLE contact_alias; DROP TABLE contact_address; DROP TABLE contact;
                     PRAGMA user_version = 14;",
                )
                .expect("back to version 14");
            connection
                .execute(
                    "INSERT INTO note (id, body, origin_kind, origin_id, created_at, updated_at, updated_by)
                     VALUES ('nt_1', 'presupuesto 1.250 €', 'alfred', 'ag_1',
                             '2026-10-28T10:00:00Z', '2026-10-28T10:00:00Z', 'alfred')",
                    [],
                )
                .expect("a note from version 14");
        }

        let (connection, _) = open(&path).expect("reopen to version 15");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        for table in ["contact", "contact_address", "contact_alias"] {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect(table);
        }
        let body: String = connection
            .query_row("SELECT body FROM note WHERE id = 'nt_1'", [], |row| {
                row.get(0)
            })
            .expect("the note is still there");
        assert_eq!(body, "presupuesto 1.250 €");
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A version 15 address has no origin. Version 16 keeps it and marks it Alfred's.
    #[test]
    fn a_version_fifteen_address_keeps_its_row_and_is_marked_as_alfreds() {
        let dir = TestDir::new("schema-v15-source");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "INSERT INTO contact (id, name, created_at, updated_at, updated_by)
                     VALUES ('ct_1', 'Ana', '2026-10-28T10:00:00Z', '2026-10-28T10:00:00Z', 'alfred');
                     INSERT INTO contact_address (contact_id, address)
                     VALUES ('ct_1', 'ana@example.com');
                     ALTER TABLE contact_address DROP COLUMN source;
                     PRAGMA user_version = 15;",
                )
                .expect("back to version 15");
        }

        let (connection, _) = open(&path).expect("reopen to version 16");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        let (address, source): (String, String) = connection
            .query_row(
                "SELECT address, source FROM contact_address WHERE contact_id = 'ct_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("the address is still there");
        assert_eq!(
            (address.as_str(), source.as_str()),
            ("ana@example.com", "alfred")
        );
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The source admits the four origins an address can have, and no other word.
    #[test]
    fn the_source_admits_alfreds_the_users_the_phone_and_the_app_only() {
        let (connection, _) = open_in_memory().expect("schema");
        connection
            .execute(
                "INSERT INTO contact (id, name, created_at, updated_at, updated_by)
                 VALUES ('ct_1', 'Ana', '2026-10-28T10:00:00Z', '2026-10-28T10:00:00Z', 'alfred')",
                [],
            )
            .expect("contact");
        for source in ["alfred", "user_message", "phone", "app"] {
            connection
                .execute(
                    "INSERT INTO contact_address (contact_id, address, source)
                     VALUES ('ct_1', ?1, ?2)",
                    params![format!("{source}@example.com"), source],
                )
                .expect(source);
        }
        let refused = connection.execute(
            "INSERT INTO contact_address (contact_id, address, source)
             VALUES ('ct_1', 'otro@example.com', 'mail')",
            [],
        );
        assert!(refused.is_err(), "an unknown origin is refused");
    }

    /// A version 16 database admits three origins. Version 17 rebuilds the table so
    /// that `app` is admitted too, keeps every address and its origin, and refuses
    /// the same unknown word as before.
    #[test]
    fn a_version_sixteen_database_admits_the_app_origin_and_keeps_its_addresses() {
        let dir = TestDir::new("schema-v16-app-source");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current schema");
            connection
                .execute_batch(
                    "INSERT INTO contact (id, name, created_at, updated_at, updated_by)
                     VALUES ('ct_1', 'Ana', '2026-10-28T10:00:00Z', '2026-10-28T10:00:00Z', 'alfred');
                     DROP TABLE contact_address;
                     CREATE TABLE contact_address (
                         contact_id  TEXT NOT NULL REFERENCES contact(id) ON DELETE CASCADE,
                         address     TEXT NOT NULL,
                         label       TEXT,
                         source      TEXT NOT NULL DEFAULT 'alfred'
                                     CHECK (source IN ('alfred', 'user_message', 'phone')),
                         UNIQUE (address)
                     );
                     INSERT INTO contact_address (contact_id, address, source)
                     VALUES ('ct_1', 'ana@example.com', 'phone');
                     PRAGMA user_version = 16;",
                )
                .expect("back to version 16");
        }

        let (connection, _) = open(&path).expect("reopen to version 17");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        let kept: (String, String) = connection
            .query_row(
                "SELECT address, source FROM contact_address WHERE contact_id = 'ct_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("the address is still there");
        assert_eq!(kept, ("ana@example.com".to_string(), "phone".to_string()));
        connection
            .execute(
                "INSERT INTO contact_address (contact_id, address, source)
                 VALUES ('ct_1', 'app@example.com', 'app')",
                [],
            )
            .expect("the app origin is admitted");
        assert!(connection
            .execute(
                "INSERT INTO contact_address (contact_id, address, source)
                 VALUES ('ct_1', 'otro@example.com', 'mail')",
                [],
            )
            .is_err());
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The rebuild runs again without a change: a second pass keeps the rows and
    /// the table as they are.
    #[test]
    fn the_app_origin_migration_runs_again_without_changing_anything() {
        let (connection, _) = open_in_memory().expect("schema");
        connection
            .execute(
                "INSERT INTO contact (id, name, created_at, updated_at, updated_by)
                 VALUES ('ct_1', 'Ana', '2026-10-28T10:00:00Z', '2026-10-28T10:00:00Z', 'alfred')",
                [],
            )
            .expect("contact");
        connection
            .execute(
                "INSERT INTO contact_address (contact_id, address, source)
                 VALUES ('ct_1', 'ana@example.com', 'app')",
                [],
            )
            .expect("address");

        admit_app_source(&connection).expect("second pass");
        admit_app_source(&connection).expect("third pass");
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM contact_address", [], |row| row.get(0))
            .expect("count");
        assert_eq!(rows, 1);
    }
}
