//! The tables of the team surface (CORE7): projects keep one lead,
//! delegations form the Alfred -> lead -> specialist tree, and a card is the
//! timeline entry that points at one delegation. `seq` is shared with
//! messages so both interleave in order.

pub const TEAM_SCHEMA: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS project_root ON project(root_path);

CREATE UNIQUE INDEX IF NOT EXISTS one_lead_per_project
    ON agent_seat(project_id) WHERE role_id = 'lead' AND project_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS delegation (
    id                TEXT PRIMARY KEY,
    company_id        TEXT NOT NULL REFERENCES company(id),
    project_id        TEXT REFERENCES project(id),
    conversation_id   TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    parent_id         TEXT REFERENCES delegation(id),
    from_agent_id     TEXT NOT NULL REFERENCES agent_seat(id),
    to_agent_id       TEXT NOT NULL REFERENCES agent_seat(id),
    task              TEXT NOT NULL,
    digest            TEXT NOT NULL,
    state             TEXT NOT NULL,
    depth             INTEGER NOT NULL,
    turn_key          TEXT NOT NULL,
    run_id            TEXT REFERENCES run(id),
    pane_id           TEXT,
    result_message_id TEXT REFERENCES message(id),
    reason            TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    finished_at       TEXT
);

CREATE INDEX IF NOT EXISTS delegation_active ON delegation(state);
CREATE INDEX IF NOT EXISTS delegation_to ON delegation(to_agent_id, state);
CREATE INDEX IF NOT EXISTS delegation_pane ON delegation(pane_id, state);
CREATE INDEX IF NOT EXISTS delegation_cycle
    ON delegation(from_agent_id, to_agent_id, digest, created_at);
CREATE INDEX IF NOT EXISTS delegation_hour ON delegation(from_agent_id, created_at);
CREATE INDEX IF NOT EXISTS delegation_turn ON delegation(from_agent_id, turn_key);

CREATE TABLE IF NOT EXISTS timeline_card (
    id            TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    delegation_id TEXT NOT NULL UNIQUE REFERENCES delegation(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    title         TEXT NOT NULL,
    summary       TEXT NOT NULL,
    status        TEXT NOT NULL,
    agent_id      TEXT NOT NULL,
    pane_id       TEXT,
    attention     INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE(conversation_id, seq)
);

CREATE TABLE IF NOT EXISTS delegation_synthesis (
    delegation_id     TEXT PRIMARY KEY REFERENCES delegation(id) ON DELETE CASCADE,
    conversation_id   TEXT NOT NULL,
    result_message_id TEXT NOT NULL REFERENCES message(id),
    state             TEXT NOT NULL,
    run_id            TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);
"#;
