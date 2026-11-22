//! The base schema: every table the chat store needs on a fresh database. The
//! migrations in `schema.rs` only add to it.

pub(crate) const BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS company (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project (
    id            TEXT PRIMARY KEY,
    company_id    TEXT NOT NULL REFERENCES company(id),
    name          TEXT NOT NULL,
    root_path     TEXT NOT NULL,
    lead_agent_id TEXT,
    status        TEXT NOT NULL DEFAULT 'active',
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS section (
    id         TEXT PRIMARY KEY,
    company_id TEXT NOT NULL REFERENCES company(id),
    name       TEXT NOT NULL,
    position   INTEGER NOT NULL,
    archived   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_seat (
    id          TEXT PRIMARY KEY,
    company_id  TEXT NOT NULL REFERENCES company(id),
    project_id  TEXT REFERENCES project(id),
    role_id     TEXT NOT NULL,
    name        TEXT NOT NULL,
    avatar      TEXT,
    kind        TEXT NOT NULL DEFAULT 'unset',
    model       TEXT,
    pane_id     TEXT,
    prompt_hash TEXT,
    status      TEXT NOT NULL DEFAULT 'draft',
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversation (
    id              TEXT PRIMARY KEY,
    company_id      TEXT NOT NULL REFERENCES company(id),
    project_id      TEXT REFERENCES project(id),
    title           TEXT NOT NULL,
    pinned          INTEGER NOT NULL DEFAULT 0,
    section_id      TEXT REFERENCES section(id),
    archived        INTEGER NOT NULL DEFAULT 0,
    last_message_at TEXT NOT NULL,
    last_read_seq   INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS message (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    author_kind     TEXT NOT NULL,
    author_agent_id TEXT REFERENCES agent_seat(id),
    body_md         TEXT NOT NULL,
    state           TEXT NOT NULL,
    client_id       TEXT,
    created_at      TEXT NOT NULL,
    UNIQUE(conversation_id, seq)
);

CREATE UNIQUE INDEX IF NOT EXISTS message_client_id
    ON message(conversation_id, client_id) WHERE client_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS message_timeline ON message(conversation_id, seq);
CREATE INDEX IF NOT EXISTS conversation_recent
    ON conversation(company_id, archived, pinned, last_message_at);

CREATE TABLE IF NOT EXISTS run (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL REFERENCES agent_seat(id),
    pane_id     TEXT,
    message_id  TEXT REFERENCES message(id),
    state       TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    reason      TEXT
);

CREATE INDEX IF NOT EXISTS run_message ON run(message_id);
CREATE INDEX IF NOT EXISTS run_state ON run(state);

CREATE TABLE IF NOT EXISTS chat_event (
    id              INTEGER PRIMARY KEY,
    event           TEXT NOT NULL,
    conversation_id TEXT,
    body_json       TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
"#;
