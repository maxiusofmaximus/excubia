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
use crate::schema_agenda::AGENDA_SCHEMA;
use crate::schema_approval::{add_policy_columns, APPROVAL_SCHEMA, HOOK_TOKEN_SCHEMA, MODE_SCHEMA};
use crate::schema_attachment::ATTACHMENT_SCHEMA;
use crate::schema_base::BASE_SCHEMA;
use crate::schema_contact::{add_source_column, admit_app_source, CONTACT_SCHEMA};
use crate::schema_hire::{add_hire_columns, HIRE_SCHEMA};
use crate::schema_mail::{MAIL_CARD_EXPIRY_SCHEMA, MAIL_DRAFT_SCHEMA, MAIL_SCHEMA};
use crate::schema_progress::RUN_PROGRESS_SCHEMA;
use crate::schema_reply::MESSAGE_REPLY_SCHEMA;
use crate::schema_team::TEAM_SCHEMA;

pub const SCHEMA_VERSION: i64 = 25;

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
    mark_interrupted(&mut connection)?;
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
        transaction.pragma_update(None, "user_version", 6)?;
        transaction.commit()?;
    }
    if version < 7 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MAIL_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 7)?;
        transaction.commit()?;
    }
    if version < 8 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MAIL_CARD_EXPIRY_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 8)?;
        transaction.commit()?;
    }
    if version < 9 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(HIRE_SCHEMA)?;
        add_hire_columns(&transaction)?;
        transaction.pragma_update(None, "user_version", 9)?;
        transaction.commit()?;
    }
    if version < 10 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MAIL_DRAFT_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 10)?;
        transaction.commit()?;
    }
    if version < 11 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MESSAGE_REPLY_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 11)?;
        transaction.commit()?;
    }
    if version < 12 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(RUN_PROGRESS_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 12)?;
        transaction.commit()?;
    }
    if version < 13 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(ATTACHMENT_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 13)?;
        transaction.commit()?;
    }
    if version < 14 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(AGENDA_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 14)?;
        transaction.commit()?;
    }
    if version < 15 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(CONTACT_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 15)?;
        transaction.commit()?;
    }
    if version < 16 {
        let transaction = connection.transaction()?;
        add_source_column(&transaction)?;
        transaction.pragma_update(None, "user_version", 16)?;
        transaction.commit()?;
    }
    if version < 17 {
        let transaction = connection.transaction()?;
        admit_app_source(&transaction)?;
        transaction.pragma_update(None, "user_version", 17)?;
        transaction.commit()?;
    }
    if version < 18 {
        crate::schema_calendar::migrate_to_18(connection)?;
    }
    if version < 19 {
        crate::schema_recurrence::migrate_to_19(connection)?;
    }
    if version < 20 {
        crate::schema_ics::migrate_to_20(connection)?;
    }
    if version < 21 {
        crate::schema_alert::migrate_to_21(connection)?;
    }
    if version < 22 {
        crate::schema_plan::migrate_to_22(connection)?;
    }
    if version < 23 {
        crate::schema_recipe::migrate_to_23(connection)?;
    }
    if version < 24 {
        crate::schema_plan::migrate_to_24(connection)?;
    }
    if version < 25 {
        crate::schema_schedule::migrate_to_25(connection)?;
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
fn mark_interrupted(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    let now = now(&transaction)?;
    // The user turns that were open journal their `updated` frame with the close.
    let run_ids = crate::store::open_run_ids(&transaction)?;
    transaction.execute(
        "UPDATE run
            SET state = 'interrupted', finished_at = ?1, reason = 'host_restart'
          WHERE state IN ('running', 'queued')",
        [&now],
    )?;
    crate::store::journal_turns(&transaction, run_ids.iter().map(String::as_str))?;
    transaction.execute(
        "UPDATE delegation
            SET state = 'interrupted', finished_at = ?1, updated_at = ?1,
                reason = 'host_restart'
          WHERE state IN ('running', 'waiting')",
        [&now],
    )?;
    transaction.execute(
        "UPDATE timeline_card
            SET status = 'interrupted', pane_id = NULL, updated_at = ?1
          WHERE status IN ('running', 'waiting')",
        [&now],
    )?;
    transaction.execute(
        "UPDATE agent_seat
            SET pane_id = NULL, status = 'stopped'
          WHERE role_id != 'alfred' AND pane_id IS NOT NULL",
        [],
    )?;
    // No token outlives the process that owned its run or panel.
    transaction.execute(
        "UPDATE hook_token SET revoked_at = ?1 WHERE revoked_at IS NULL",
        [&now],
    )?;
    transaction.commit()?;
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
#[path = "schema_tests.rs"]
mod tests;
