//! The schedule tables (F4-CORE): one row per scheduled text, and the history of its due
//! instants. A schedule names its conversation and its oficio; the runs keep at most
//! `HISTORY_KEEP` rows each, trimmed by the store when a run is recorded.

use rusqlite::Connection;

use crate::error::ChatError;

pub const SCHEDULE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schedule (
    id              TEXT PRIMARY KEY,
    company_id      TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    role_id         TEXT NOT NULL,
    title           TEXT NOT NULL,
    body            TEXT,
    recipe_id       TEXT,
    rule            TEXT,
    start_date      TEXT NOT NULL,
    time_local      TEXT NOT NULL,
    timezone        TEXT NOT NULL,
    steps           INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL,
    plan_id         TEXT,
    next_run_at     TEXT,
    last_run_at     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    CHECK ((body IS NULL) != (recipe_id IS NULL))
);

CREATE INDEX IF NOT EXISTS schedule_due ON schedule(status, next_run_at);
CREATE INDEX IF NOT EXISTS schedule_role ON schedule(role_id, created_at);

CREATE TABLE IF NOT EXISTS schedule_run (
    id            TEXT PRIMARY KEY,
    schedule_id   TEXT NOT NULL,
    due_at        TEXT NOT NULL,
    started_at    TEXT,
    outcome       TEXT NOT NULL,
    delegation_id TEXT
);

CREATE INDEX IF NOT EXISTS schedule_run_by ON schedule_run(schedule_id, due_at);
"#;

pub fn migrate_to_25(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(SCHEDULE_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 25)?;
    transaction.commit()?;
    Ok(())
}
