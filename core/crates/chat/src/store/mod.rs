//! The chat repository: one SQLite connection behind a mutex, one broadcast
//! channel for the SSE forwarder, and an append-only event journal that makes
//! `Last-Event-ID` resumption possible.

mod agenda;
mod alerts;
mod approval_create;
mod approval_decisions;
mod approval_delegation;
mod approval_exclusion;
mod approval_level;
mod approval_mode;
mod approval_policy;
mod approval_rules;
mod approval_settle;
mod approval_timeline;
mod approvals;
mod attachments;
mod bootstrap;
mod calendar;
mod calendar_card;
mod calendar_ics;
#[cfg(test)]
#[path = "calendar_ics_tests.rs"]
mod calendar_ics_tests;
mod calendar_read;
#[cfg(test)]
#[path = "calendar_recurrence_tests.rs"]
mod calendar_recurrence_tests;
mod calendar_remind;
#[cfg(test)]
#[path = "calendar_remind_tests.rs"]
mod calendar_remind_tests;
mod calendar_series;
#[cfg(test)]
#[path = "calendar_tests.rs"]
mod calendar_tests;
mod calendar_time;
mod contacts;
mod contacts_admit;
mod contacts_import;
mod contacts_search;
mod context;
mod conversation_delete;
mod conversations;
mod delegation_rules;
mod delegation_state;
mod delegations;
#[cfg(test)]
#[path = "device_tests.rs"]
mod device_tests;
mod hires;
mod hook_tokens;
mod journal;
mod lookup;
mod mail_card;
mod mail_draft_card;
mod mail_history;
mod messages;
mod notes;
mod projects;
mod runs;
mod seats;
mod sections;
mod status;
mod synthesis;

pub use alerts::{AlertSummary, AlertWriter};
use bootstrap::root_label;
pub use calendar::EventOrigin;
pub use calendar_read::{CalendarSummary, NextEvent};
pub(crate) use journal::{append_event, journal_turns, revision};
pub(crate) use lookup::{
    alfred_id, company_id, conversation_exists, next_seq, open_run_ids, run_info, seat_summary,
    turn_for_message,
};
pub use mail_card::NewMailCard;
pub use mail_draft_card::MailDraftCard;
pub use mail_history::SenderHistory;
pub use runs::{validate_suggestions, ProgressOutcome};
pub use status::{StatusApproval, StatusDelegation, StatusFacts};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::{params, Connection};
use tokio::sync::broadcast;

use crate::attachments::AttachmentFiles;
use crate::clock::{self, Clock, SystemClock};
use crate::error::ChatError;
use crate::model::ChatFrame;
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
    /// The folder of the attachment files, known once the host knows its data dir.
    attachment_files: Mutex<Option<AttachmentFiles>>,
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
            attachment_files: Mutex::new(None),
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
    pub fn now_text(&self) -> String {
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

    /// Where the attachment files live: `<root>/<conversation_id>/<attachment_id>`.
    pub fn set_attachments_root(&self, root: &Path) {
        *self
            .attachment_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(AttachmentFiles::new(root));
    }

    pub fn attachment_files(&self) -> Option<AttachmentFiles> {
        self.attachment_files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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

    pub(super) fn publish_all(&self, frames: Vec<ChatFrame>) {
        for frame in frames {
            self.publish(frame);
        }
    }
}
