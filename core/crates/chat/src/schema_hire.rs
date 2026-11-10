//! The tables of the hires and of the onboarding state (ONBOARD-T1). A hire
//! lives apart from `agent_seat`. Its seat link is a nullable column: it stays
//! empty until the first assignment, which this schema does not write.

use rusqlite::Connection;

use crate::schema_approval::add_column_once;

pub const HIRE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS agent_hire (
    id           TEXT PRIMARY KEY,
    company_id   TEXT NOT NULL REFERENCES company(id),
    catalog_role TEXT NOT NULL,
    area         TEXT NOT NULL,
    name         TEXT NOT NULL,
    avatar       TEXT,
    engine       TEXT,
    profile      TEXT,
    skills_allow TEXT NOT NULL DEFAULT '[]',
    status       TEXT NOT NULL DEFAULT 'hired' CHECK (status IN ('hired', 'removed')),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    removed_at   TEXT
);

-- A role is hired once at a time; a dismissed hire does not block the role.
CREATE UNIQUE INDEX IF NOT EXISTS agent_hire_active_role
    ON agent_hire(catalog_role) WHERE status = 'hired';
CREATE INDEX IF NOT EXISTS agent_hire_area ON agent_hire(area, status);
"#;

/// The seat link of a hire. The column is added only when it is missing, so the
/// migration can run again (the same rule as the approval columns).
pub fn add_hire_columns(connection: &Connection) -> Result<(), rusqlite::Error> {
    add_column_once(
        connection,
        "agent_seat",
        "hire_id",
        "TEXT REFERENCES agent_hire(id)",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{open, open_in_memory, SCHEMA_VERSION};
    use crate::schema_approval::{
        add_policy_columns, APPROVAL_SCHEMA, HOOK_TOKEN_SCHEMA, MODE_SCHEMA,
    };
    use crate::schema_base::BASE_SCHEMA;
    use crate::schema_mail::{MAIL_CARD_EXPIRY_SCHEMA, MAIL_SCHEMA};
    use crate::schema_team::TEAM_SCHEMA;
    use excubia_core_test_dir::TestDir;

    /// A database written by version 8 keeps its seats, gains the hire table and
    /// the seat link empty, and the migration can run again without change.
    #[test]
    fn a_version_eight_database_gains_the_hires_and_keeps_its_seats() {
        let dir = TestDir::new("schema-v8-hires");
        let path = dir.join("chat.db");
        {
            let legacy = Connection::open(&path).expect("legacy database");
            for schema in [
                BASE_SCHEMA,
                TEAM_SCHEMA,
                APPROVAL_SCHEMA,
                HOOK_TOKEN_SCHEMA,
                MODE_SCHEMA,
                MAIL_SCHEMA,
                MAIL_CARD_EXPIRY_SCHEMA,
            ] {
                legacy.execute_batch(schema).expect("schema");
            }
            add_policy_columns(&legacy).expect("policy columns");
            legacy
                .execute_batch(
                    "INSERT INTO company (id, name, created_at)
                        VALUES ('co_1', 'Legacy', '2026-10-22T09:00:00.000Z');
                     INSERT INTO agent_seat
                        (id, company_id, project_id, role_id, name, avatar, kind, status, created_at)
                        VALUES ('ag_1', 'co_1', NULL, 'alfred', 'Alfred', 'alfred', 'unset',
                                'draft', '2026-10-22T09:00:00.000Z');",
                )
                .expect("legacy rows");
            legacy
                .pragma_update(None, "user_version", 8)
                .expect("version eight");
        }

        let (connection, _) = open(&path).expect("migrated");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        let seat_hire: Option<String> = connection
            .query_row(
                "SELECT hire_id FROM agent_seat WHERE id = 'ag_1'",
                [],
                |row| row.get(0),
            )
            .expect("seat");
        assert_eq!(seat_hire, None);
        let hires: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_hire", [], |row| row.get(0))
            .expect("hires");
        assert_eq!(hires, 0);

        add_hire_columns(&connection).expect("the column can be added again");
        connection
            .execute_batch(HIRE_SCHEMA)
            .expect("the hire schema can run again");
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The link of a seat points at an existing hire and nothing else.
    #[test]
    fn a_seat_links_only_to_an_existing_hire() {
        let (connection, _) = open_in_memory().expect("open");
        connection
            .execute(
                "INSERT INTO agent_seat
                    (id, company_id, project_id, role_id, name, kind, status, created_at)
                 VALUES ('ag_1', (SELECT id FROM company), NULL, 'specialist', 'Ines',
                         'unset', 'draft', '2026-10-25T09:00:00.000Z')",
                [],
            )
            .expect("seat");
        let dangling = connection.execute(
            "UPDATE agent_seat SET hire_id = 'hire_missing' WHERE id = 'ag_1'",
            [],
        );
        assert!(dangling.is_err(), "the foreign key refuses an unknown hire");

        connection
            .execute(
                "INSERT INTO agent_hire
                    (id, company_id, catalog_role, area, name, created_at, updated_at)
                 VALUES ('hire_1', (SELECT id FROM company), 'audio-editor', 'audio', 'Ines',
                         '2026-10-25T09:00:00.000Z', '2026-10-25T09:00:00.000Z')",
                [],
            )
            .expect("hire");
        connection
            .execute(
                "UPDATE agent_seat SET hire_id = 'hire_1' WHERE id = 'ag_1'",
                [],
            )
            .expect("linked");
    }
}
