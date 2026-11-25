//! The plan table (F3): one row per plan an agent proposes. `steps` is the JSON
//! of the steps; `delegation_id` is set once a delegation starts from the plan.

use rusqlite::Connection;

use crate::error::ChatError;

pub const PLAN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS plan (
    id                TEXT PRIMARY KEY,
    conversation_id   TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    proposer_agent_id TEXT NOT NULL REFERENCES agent_seat(id),
    steps             TEXT NOT NULL,
    status            TEXT NOT NULL,
    note              TEXT,
    delegation_id     TEXT REFERENCES delegation(id) ON DELETE SET NULL,
    decided_by        TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    decided_at        TEXT
);

CREATE INDEX IF NOT EXISTS plan_status ON plan(status, created_at);
"#;

pub fn migrate_to_22(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(PLAN_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 22)?;
    transaction.commit()?;
    Ok(())
}

/// The timeline lines of plans: one per status a plan reaches, numbered in the
/// conversation's `seq` order with its messages and approvals (F3 timeline).
pub const TIMELINE_PLAN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS timeline_plan (
    plan_id         TEXT NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    status          TEXT NOT NULL,
    summary         TEXT NOT NULL,
    decider_kind    TEXT,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (plan_id, status),
    UNIQUE (conversation_id, seq)
);
"#;

pub fn migrate_to_24(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(TIMELINE_PLAN_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 24)?;
    transaction.commit()?;
    Ok(())
}

/// The resumption of a plan the agent asked to run on approval (`run_on_approval`).
/// One row per plan, so a repeated answer can never resume it twice. States:
/// `armed` (proposed, waiting), `pending` (approved, not yet handed to Alfred),
/// `queued` (its turn is in Alfred's queue, `run_id` names it), `skipped` (refused,
/// archived, or already linked to a delegation).
pub const PLAN_RESUME_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS plan_resume (
    plan_id         TEXT PRIMARY KEY REFERENCES plan(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    state           TEXT NOT NULL,
    run_id          TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS plan_resume_state ON plan_resume(state);
"#;

pub fn migrate_to_30(connection: &mut Connection) -> Result<(), ChatError> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(PLAN_RESUME_SCHEMA)?;
    transaction.pragma_update(None, "user_version", 30)?;
    transaction.commit()?;
    Ok(())
}
