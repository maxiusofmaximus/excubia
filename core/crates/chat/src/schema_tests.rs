use super::*;
use excubia_core_test_dir::TestDir;

#[test]
fn migrations_run_once_and_keep_the_seed() {
    let (mut connection, _) = open_in_memory().expect("open");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    let companies: i64 = connection
        .query_row("SELECT COUNT(*) FROM company", [], |row| row.get(0))
        .expect("companies");
    assert_eq!(companies, 1);
    migrate(&mut connection).expect("re-run");
    let seats: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_seat", [], |row| row.get(0))
        .expect("seats");
    assert_eq!(seats, 2, "Alfred and Ernesto are seeded once");
}

/// A database written before the policy (schema 4) keeps its approvals and
/// gains the new columns empty; running the columns again changes nothing.
#[test]
fn a_version_four_database_gains_the_policy_columns_and_keeps_its_rows() {
    let dir = TestDir::new("schema-v4");
    let path = dir.join("chat.db");
    {
        let legacy = Connection::open(&path).expect("legacy database");
        legacy.execute_batch(BASE_SCHEMA).expect("base");
        legacy.execute_batch(TEAM_SCHEMA).expect("team");
        legacy.execute_batch(APPROVAL_SCHEMA).expect("approval");
        legacy
            .execute_batch(HOOK_TOKEN_SCHEMA)
            .expect("hook tokens");
        legacy
            .execute_batch(
                "INSERT INTO company (id, name, created_at)
                    VALUES ('co_1', 'Legacy', '2026-10-22T09:00:00.000Z');
                 INSERT INTO approval
                    (id, company_id, kind, tool, summary, risk, action_digest, status,
                     expires_at, created_at, updated_at)
                 VALUES ('apr_old', 'co_1', 'command', 'bash', 'cargo test', 'high',
                         'sha256:old', 'approved', '2026-10-22T10:00:00.000Z',
                         '2026-10-22T09:58:00.000Z', '2026-10-22T09:58:00.000Z');",
            )
            .expect("old approval");
        legacy
            .pragma_update(None, "user_version", 4)
            .expect("version four");
    }

    let (connection, _) = open(&path).expect("migrated");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    let (level, status): (Option<String>, String) = connection
        .query_row(
            "SELECT level, status FROM approval WHERE id = 'apr_old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("old row");
    assert_eq!(level, None, "old rows keep NULL in the new columns");
    assert_eq!(status, "approved");
    add_policy_columns(&connection).expect("the columns can be added again");
    drop(connection);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn search_capability_is_recorded() {
    let (connection, mode) = open_in_memory().expect("open");
    let recorded = meta_value(&connection, "search_mode")
        .expect("meta")
        .expect("mode");
    assert_eq!(recorded, mode.label());
}
