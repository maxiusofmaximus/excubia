//! Schema, versioned migrations and the seed of the single-company host.
//!
//! `PRAGMA user_version` is the migration counter. Every version is applied
//! inside one transaction, so a crash leaves either the old or the new schema.
//! Search is capability-based: FTS5 when the bundled SQLite has it, `LIKE`
//! otherwise; the contract does not change.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection};

use crate::error::ChatError;
use crate::id;
use crate::schema_approval::{add_policy_columns, APPROVAL_SCHEMA, HOOK_TOKEN_SCHEMA, MODE_SCHEMA};
use crate::schema_base::BASE_SCHEMA;
use crate::schema_team::TEAM_SCHEMA;

pub const SCHEMA_VERSION: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Fts5,
    Like,
}

impl SearchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fts5 => "fts5",
            Self::Like => "like",
        }
    }
}

/// Open a file database, apply migrations and mark interrupted turns.
pub fn open(path: &Path) -> Result<(Connection, SearchMode), ChatError> {
    let connection = Connection::open(path)?;
    initialise(connection)
}

pub fn open_in_memory() -> Result<(Connection, SearchMode), ChatError> {
    initialise(Connection::open_in_memory()?)
}

fn initialise(mut connection: Connection) -> Result<(Connection, SearchMode), ChatError> {
    connection.busy_timeout(Duration::from_millis(5_000))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    // WAL is persistent per database; on an in-memory database SQLite answers
    // "memory" instead of failing.
    let _: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    migrate(&mut connection)?;
    let search = ensure_search(&connection)?;
    mark_interrupted(&connection)?;
    Ok((connection, search))
}

fn migrate(connection: &mut Connection) -> Result<(), ChatError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    if version < 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(BASE_SCHEMA)?;
        seed(&transaction)?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
    }
    if version < 2 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(TEAM_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 2)?;
        transaction.commit()?;
    }
    if version < 3 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(APPROVAL_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 3)?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(HOOK_TOKEN_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 4)?;
        transaction.commit()?;
    }
    if version < 5 {
        let transaction = connection.transaction()?;
        add_policy_columns(&transaction)?;
        transaction.pragma_update(None, "user_version", 5)?;
        transaction.commit()?;
    }
    if version < 6 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MODE_SCHEMA)?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
    }
    Ok(())
}

fn seed(connection: &Connection) -> Result<(), ChatError> {
    let companies: i64 =
        connection.query_row("SELECT COUNT(*) FROM company", [], |row| row.get(0))?;
    if companies > 0 {
        return Ok(());
    }
    let now = now(connection)?;
    let company = id::new_id("co");
    connection.execute(
        "INSERT INTO company (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![company, "Mi empresa", now],
    )?;
    let alfred = id::new_id("ag");
    connection.execute(
        "INSERT INTO agent_seat
            (id, company_id, project_id, role_id, name, avatar, kind, status, created_at)
         VALUES (?1, ?2, NULL, 'alfred', 'Alfred', 'alfred', 'unset', 'draft', ?3)",
        params![alfred, company, now],
    )?;
    // Ernesto is the default lead of a future project. No project is invented
    // at first boot (`orchestrator-chat.md` Â§1.1), so the seat stays unassigned.
    let ernesto = id::new_id("ag");
    connection.execute(
        "INSERT INTO agent_seat
            (id, company_id, project_id, role_id, name, avatar, kind, status, created_at)
         VALUES (?1, ?2, NULL, 'lead', 'Ernesto', 'ernesto', 'unset', 'draft', ?3)",
        params![ernesto, company, now],
    )?;
    connection.execute("INSERT INTO meta (key, value) VALUES ('revision', '0')", [])?;
    Ok(())
}

fn ensure_search(connection: &Connection) -> Result<SearchMode, ChatError> {
    if let Some(mode) = meta_value(connection, "search_mode")? {
        if mode == SearchMode::Like.label() {
            return Ok(SearchMode::Like);
        }
    }
    match connection.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS conversation_fts
         USING fts5(conversation_id UNINDEXED, title, body);
         DELETE FROM conversation_fts;",
    ) {
        Ok(()) => {
            rebuild_search(connection)?;
            set_meta(connection, "search_mode", SearchMode::Fts5.label())?;
            Ok(SearchMode::Fts5)
        }
        Err(error) => {
            eprintln!("chat: FTS5 unavailable ({error}); search falls back to LIKE");
            set_meta(connection, "search_mode", SearchMode::Like.label())?;
            Ok(SearchMode::Like)
        }
    }
}

fn rebuild_search(connection: &Connection) -> Result<(), ChatError> {
    let ids: Vec<String> = {
        let mut statement = connection.prepare("SELECT id FROM conversation")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    for id in ids {
        reindex(connection, &id)?;
    }
    Ok(())
}

/// Keep the FTS row of one conversation in sync with its title and messages.
pub fn reindex(connection: &Connection, conversation_id: &str) -> Result<(), ChatError> {
    let title: String = match connection.query_row(
        "SELECT title FROM conversation WHERE id = ?1",
        [conversation_id],
        |row| row.get(0),
    ) {
        Ok(title) => title,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let body: String = connection.query_row(
        "SELECT COALESCE(group_concat(body_md, char(10)), '')
           FROM (SELECT body_md FROM message WHERE conversation_id = ?1 ORDER BY seq)",
        [conversation_id],
        |row| row.get(0),
    )?;
    connection.execute(
        "DELETE FROM conversation_fts WHERE conversation_id = ?1",
        [conversation_id],
    )?;
    connection.execute(
        "INSERT INTO conversation_fts (conversation_id, title, body) VALUES (?1, ?2, ?3)",
        params![conversation_id, title, body],
    )?;
    Ok(())
}

/// A turn is durable only while this process lives: the seat queue and the MCP
/// token die with it. Closing both `running` and `queued` at startup is what
/// prevents a restart from duplicating a turn or leaving it pending forever.
///
/// CORE7 applies the same rule to delegations (`running`/`waiting` were in the
/// hands of a process that no longer exists) and forgets the pane of every
/// project seat: panels are never relaunched by themselves.
///
/// The pending approvals are not closed here: they expire through the store,
/// which audits them as the host's own decision (`ChatStore::open`).
fn mark_interrupted(connection: &Connection) -> Result<(), ChatError> {
    let now = now(connection)?;
    connection.execute(
        "UPDATE run
            SET state = 'interrupted', finished_at = ?1, reason = 'host_restart'
          WHERE state IN ('running', 'queued')",
        [&now],
    )?;
    connection.execute(
        "UPDATE delegation
            SET state = 'interrupted', finished_at = ?1, updated_at = ?1,
                reason = 'host_restart'
          WHERE state IN ('running', 'waiting')",
        [&now],
    )?;
    connection.execute(
        "UPDATE timeline_card
            SET status = 'interrupted', pane_id = NULL, updated_at = ?1
          WHERE status IN ('running', 'waiting')",
        [&now],
    )?;
    connection.execute(
        "UPDATE agent_seat
            SET pane_id = NULL, status = 'stopped'
          WHERE role_id != 'alfred' AND pane_id IS NOT NULL",
        [],
    )?;
    // No token outlives the process that owned its run or panel.
    connection.execute(
        "UPDATE hook_token SET revoked_at = ?1 WHERE revoked_at IS NULL",
        [&now],
    )?;
    Ok(())
}

pub fn now(connection: &Connection) -> Result<String, rusqlite::Error> {
    connection.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
        row.get(0)
    })
}

pub fn meta_value(connection: &Connection, key: &str) -> Result<Option<String>, ChatError> {
    match connection.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get(0)
    }) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn set_meta(connection: &Connection, key: &str, value: &str) -> Result<(), ChatError> {
    connection.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

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
}
