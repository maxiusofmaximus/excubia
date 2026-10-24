//! The chat repository: one SQLite connection behind a mutex, one broadcast
//! channel for the SSE forwarder, and an append-only event journal that makes
//! `Last-Event-ID` resumption possible.

mod approval_create;
mod approval_decisions;
mod approval_delegation;
mod approval_level;
mod approval_mode;
mod approval_policy;
mod approval_rules;
mod approval_settle;
mod approval_timeline;
mod approvals;
mod bootstrap;
mod context;
mod conversations;
mod delegation_rules;
mod delegation_state;
mod delegations;
mod hook_tokens;
mod messages;
mod projects;
mod runs;
mod seats;
mod sections;
mod synthesis;

use bootstrap::root_label;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::broadcast;

use crate::clock::{self, Clock, SystemClock};
use crate::error::ChatError;
use crate::model::{ChatFrame, RunState, SeatSummary, Turn};
use crate::policy::PolicySource;
use crate::schema::{self, SearchMode};

const EVENT_CAPACITY: usize = 256;

pub struct ChatStore {
    connection: Mutex<Connection>,
    search: SearchMode,
    events: broadcast::Sender<ChatFrame>,
    policy: Mutex<PolicySource>,
    /// The operator's `workspaces_root`, known once the team service starts.
    workspaces_root: Mutex<Option<PathBuf>>,
    /// The clock of the approvals: the wall clock unless a test drives it.
    clock: Mutex<Arc<dyn Clock>>,
}

impl ChatStore {
    /// Open `chat.db`; its `approvals.toml` is read from the same directory.
    /// Opening is a restart: the approvals that were waiting expire now.
    pub fn open(path: &Path) -> Result<Self, ChatError> {
        let (connection, search) = schema::open(path)?;
        let store = Self::new(connection, search, PolicySource::next_to(path));
        {
            let connection = store.lock();
            approval_settle::expire_at_start(&connection, store.now_ms())?;
        }
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, ChatError> {
        let (connection, search) = schema::open_in_memory()?;
        Ok(Self::new(connection, search, PolicySource::defaults()))
    }

    fn new(connection: Connection, search: SearchMode, policy: PolicySource) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            connection: Mutex::new(connection),
            search,
            events,
            policy: Mutex::new(policy),
            workspaces_root: Mutex::new(None),
            clock: Mutex::new(Arc::new(SystemClock)),
        }
    }

    /// Replace the clock of the approvals. Tests use it to move the escalation
    /// timers without waiting; the host keeps the wall clock.
    pub fn set_clock(&self, clock: Arc<dyn Clock>) {
        *self.clock.lock().unwrap_or_else(PoisonError::into_inner) = clock;
    }

    /// The instant of the approvals, in milliseconds.
    pub(super) fn now_ms(&self) -> i64 {
        let clock = Arc::clone(&self.clock.lock().unwrap_or_else(PoisonError::into_inner));
        clock.now_ms()
    }

    /// The instant of the approvals, as the text the rows store.
    pub(super) fn now_text(&self) -> String {
        clock::iso(self.now_ms())
    }

    /// Where the operator keeps its projects; the `workspace` scope of a rule
    /// means "under this folder".
    pub fn set_workspaces_root(&self, root: &Path) {
        *self
            .workspaces_root
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(root.to_path_buf());
    }

    pub fn search_mode(&self) -> SearchMode {
        self.search
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChatFrame> {
        self.events.subscribe()
    }

    /// Journaled events newer than `last_id`, oldest first.
    pub fn events_after(&self, last_id: i64, limit: usize) -> Result<Vec<ChatFrame>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT id, event, body_json FROM chat_event
              WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![last_id, limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut frames = Vec::new();
        for row in rows {
            let (id, event, body) = row?;
            frames.push(ChatFrame {
                id,
                event,
                body: serde_json::from_str(&body)?,
            });
        }
        Ok(frames)
    }

    pub fn revision(&self) -> Result<i64, ChatError> {
        let connection = self.lock();
        revision(&connection)
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn publish(&self, frame: ChatFrame) {
        let _ = self.events.send(frame);
    }
}

pub(super) fn revision(connection: &Connection) -> Result<i64, ChatError> {
    match schema::meta_value(connection, "revision")? {
        None => Ok(0),
        Some(value) => value
            .parse()
            .map_err(|_| ChatError::Inconsistent("chat revision is not a number")),
    }
}

/// Reserve the next global chat revision and write its journal row. The caller
/// must already hold a transaction: revision and mutation commit together.
pub(super) fn append_event(
    connection: &Connection,
    event: &str,
    conversation_id: &str,
    mut body: serde_json::Value,
) -> Result<ChatFrame, ChatError> {
    let next = revision(connection)? + 1;
    schema::set_meta(connection, "revision", &next.to_string())?;
    body["revision"] = serde_json::Value::from(next);
    connection.execute(
        "INSERT INTO chat_event (id, event, conversation_id, body_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            next,
            event,
            conversation_id,
            body.to_string(),
            schema::now(connection)?
        ],
    )?;
    Ok(ChatFrame {
        id: next,
        event: event.to_string(),
        body,
    })
}

pub(super) fn company_id(connection: &Connection) -> Result<String, ChatError> {
    connection
        .query_row("SELECT id FROM company LIMIT 1", [], |row| row.get(0))
        .optional()?
        .ok_or(ChatError::NotInitialised)
}

pub(super) fn alfred_id(connection: &Connection) -> Result<String, ChatError> {
    connection
        .query_row(
            "SELECT id FROM agent_seat WHERE role_id = 'alfred' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(ChatError::NotInitialised)
}

pub(super) fn seat_summary(
    connection: &Connection,
    role_id: &str,
) -> Result<Option<SeatSummary>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, name, status, uac_state, pane_id FROM agent_seat
              WHERE role_id = ?1 ORDER BY created_at ASC, id ASC LIMIT 1",
            [role_id],
            |row| {
                Ok(SeatSummary {
                    agent_id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    uac_state: row.get(3)?,
                    pane_id: row.get(4)?,
                })
            },
        )
        .optional()?)
}

pub(super) fn conversation_exists(
    connection: &Connection,
    conversation_id: &str,
) -> Result<bool, ChatError> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM conversation WHERE id = ?1",
            [conversation_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

pub(super) fn next_seq(connection: &Connection, conversation_id: &str) -> Result<i64, ChatError> {
    Ok(connection.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM (
             SELECT seq FROM message WHERE conversation_id = ?1
             UNION ALL
             SELECT seq FROM timeline_card WHERE conversation_id = ?1
             UNION ALL
             SELECT seq FROM timeline_approval WHERE conversation_id = ?1
         )",
        [conversation_id],
        |row| row.get(0),
    )?)
}

#[derive(Clone, Debug)]
pub(super) struct RunInfo {
    pub agent_id: String,
    pub conversation_id: String,
    pub state: RunState,
}

pub(super) fn run_info(connection: &Connection, run_id: &str) -> Result<RunInfo, ChatError> {
    connection
        .query_row(
            "SELECT r.agent_id, m.conversation_id, r.state
               FROM run r JOIN message m ON m.id = r.message_id
              WHERE r.id = ?1",
            [run_id],
            |row| {
                Ok(RunInfo {
                    agent_id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    state: RunState::from_db(&row.get::<_, String>(2)?)
                        .unwrap_or(RunState::Interrupted),
                })
            },
        )
        .optional()?
        .ok_or_else(|| ChatError::NoSuchRun(run_id.to_string()))
}

pub(super) fn turn_for_message(
    connection: &Connection,
    message_id: &str,
    conversation_id: &str,
    text: &str,
) -> Result<Turn, ChatError> {
    let (id, state, agent_id): (String, String, String) = connection
        .query_row(
            "SELECT id, state, agent_id FROM run WHERE message_id = ?1
              ORDER BY rowid DESC LIMIT 1",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ChatError::Inconsistent("message without a turn"))?;
    Ok(Turn {
        id,
        state: RunState::from_db(&state).unwrap_or(RunState::Interrupted),
        agent_id,
        conversation_id: conversation_id.to_string(),
        message_id: message_id.to_string(),
        text: text.to_string(),
    })
}
