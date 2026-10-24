//! The tables of the approvals (CORE8): one approval binds one concrete
//! action to one decision. The digest is the identity of the action, so the
//! unique index is the replay protection; the audit trail is append-only.

use rusqlite::Connection;

pub const APPROVAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS approval (
    id               TEXT PRIMARY KEY,
    company_id       TEXT NOT NULL REFERENCES company(id),
    conversation_id  TEXT REFERENCES conversation(id),
    delegation_id    TEXT REFERENCES delegation(id),
    seat_id          TEXT REFERENCES agent_seat(id),
    agent_label      TEXT,
    run_id           TEXT,
    pane_id          TEXT,
    kind             TEXT NOT NULL,
    tool             TEXT NOT NULL,
    summary          TEXT NOT NULL,
    cwd              TEXT,
    paths_json       TEXT NOT NULL DEFAULT '[]',
    risk             TEXT NOT NULL,
    action_digest    TEXT NOT NULL,
    payload_redacted TEXT,
    status           TEXT NOT NULL,
    scope            TEXT NOT NULL DEFAULT 'once',
    expires_at       TEXT NOT NULL,
    decided_by       TEXT,
    decided_at       TEXT,
    consumed_at      TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS approval_action_digest ON approval(action_digest);
CREATE INDEX IF NOT EXISTS approval_pending ON approval(status, conversation_id);
CREATE INDEX IF NOT EXISTS approval_run ON approval(run_id);
CREATE INDEX IF NOT EXISTS approval_seat ON approval(seat_id, status);

CREATE TABLE IF NOT EXISTS approval_audit (
    id            INTEGER PRIMARY KEY,
    approval_id   TEXT NOT NULL REFERENCES approval(id),
    action_digest TEXT NOT NULL,
    device_id     TEXT NOT NULL,
    decision      TEXT NOT NULL,
    scope         TEXT NOT NULL,
    decided_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS approval_audit_approval ON approval_audit(approval_id);

-- One line of the timeline per approval decided below the operator (CORE9-C).
-- It takes its `seq` from the conversation's sequence, like a message or a card.
CREATE TABLE IF NOT EXISTS timeline_approval (
    approval_id     TEXT PRIMARY KEY REFERENCES approval(id),
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    status          TEXT NOT NULL,
    level           TEXT,
    summary         TEXT NOT NULL,
    decider_kind    TEXT NOT NULL,
    decider_id      TEXT,
    created_at      TEXT NOT NULL,
    UNIQUE(conversation_id, seq)
);

ALTER TABLE agent_seat ADD COLUMN uac_state TEXT NOT NULL DEFAULT 'unchecked';
"#;

/// The hook tokens (CORE8B): one credential per run or per resident panel.
/// Only the hash is stored; the scope is what the three approval routes check.
pub const HOOK_TOKEN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS hook_token (
    token_hash      TEXT PRIMARY KEY,
    seat_id         TEXT NOT NULL,
    run_id          TEXT NOT NULL DEFAULT '',
    conversation_id TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    revoked_at      TEXT
);

CREATE INDEX IF NOT EXISTS hook_token_scope ON hook_token(seat_id, run_id);
"#;

/// The approval mode (CORE9-D, RFC §5): one row per scope, `global` or a
/// project id. The database is the source of truth; `approvals.toml` ignores
/// its `mode` key.
pub const MODE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS approval_mode (
    scope      TEXT PRIMARY KEY,
    mode       TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Every accepted change of the mode, with the device that made it. The
-- client_id is the idempotency key: a replay changes nothing.
CREATE TABLE IF NOT EXISTS approval_settings_audit (
    id           INTEGER PRIMARY KEY,
    client_id    TEXT NOT NULL UNIQUE,
    scope        TEXT NOT NULL,
    from_mode    TEXT NOT NULL,
    to_mode      TEXT NOT NULL,
    decider_kind TEXT NOT NULL,
    decider_id   TEXT NOT NULL,
    level        TEXT NOT NULL,
    reason       TEXT NOT NULL,
    decided_at   TEXT NOT NULL
);
"#;

/// The host policy's columns (CORE9-A), on `approval` and `approval_audit`.
/// Old rows keep NULL. SQLite has no `ADD COLUMN IF NOT EXISTS`, so a column
/// is added only when its table lacks it: the migration can run again.
const POLICY_COLUMNS: [(&str, &str); 8] = [
    ("level", "TEXT"),
    ("escalated_from", "TEXT"),
    ("decider_kind", "TEXT"),
    ("decider_id", "TEXT"),
    ("reason", "TEXT"),
    ("question", "TEXT"),
    ("risk_score", "REAL"),
    ("scorer_version", "TEXT"),
];

/// The deadline of the level an approval waits at (CORE9-C): 40 s per agent
/// level, NULL when the operator is the one who decides.
const ESCALATION_COLUMNS: [(&str, &str); 1] = [("level_expires_at", "TEXT")];

/// The request that answered an approval (`client_id`, the idempotency key of
/// the decision). The audit row names the device that decided.
const DECISION_COLUMNS: [(&str, &str); 1] = [("client_id", "TEXT")];

pub fn add_policy_columns(connection: &Connection) -> Result<(), rusqlite::Error> {
    for table in ["approval", "approval_audit"] {
        for (column, declaration) in POLICY_COLUMNS {
            add_column_once(connection, table, column, declaration)?;
        }
    }
    for (column, declaration) in ESCALATION_COLUMNS {
        add_column_once(connection, "approval", column, declaration)?;
    }
    for (column, declaration) in DECISION_COLUMNS {
        add_column_once(connection, "approval_audit", column, declaration)?;
    }
    Ok(())
}

fn add_column_once(
    connection: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<(), rusqlite::Error> {
    if !has_column(connection, table, column)? {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {declaration}"),
            [],
        )?;
    }
    Ok(())
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
