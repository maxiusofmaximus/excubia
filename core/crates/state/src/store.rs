//! The store: one lock, every mutation goes through it, and the reduced view
//! is derived from the records on read.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};

use crate::frame::{Frame, SharedFrame, EVENT_CLOSED, EVENT_CREATED, EVENT_STATUS, EVENT_UPDATED};
use crate::payload::{counts, rank, StatePayload, TriageRow};
use crate::persist;
use crate::record::AgentRecord;
use crate::status::Status;

const BROADCAST_CAPACITY: usize = 256;

/// The fields of a new pane record.
pub struct NewAgent {
    pub pane_id: String,
    pub workspace_id: String,
    pub project: String,
    pub agent: String,
    pub cwd: String,
    /// The operator's name for the conversation, when one was given.
    pub label: Option<String>,
    /// True for a pane restored from a previous host run: it exists in the
    /// model, its process does not.
    pub restored: bool,
}

struct Inner {
    agents: HashMap<String, AgentRecord>,
    revision: u64,
    seq: u64,
    next_pane: u32,
    counters_path: Option<PathBuf>,
}

impl Inner {
    /// Persist the monotonic counters after a mutation. Failures are logged
    /// and do not fail the mutation: the worst case is that a client sees the
    /// counters restart, which the contract only forbids within a run.
    fn persist(&self) {
        if let Some(path) = &self.counters_path {
            persist::write(path, self.revision, self.seq);
        }
    }
}

/// The store. Cheap to clone through an `Arc`.
pub struct StateStore {
    version: String,
    inner: Mutex<Inner>,
    tx: broadcast::Sender<SharedFrame>,
}

impl StateStore {
    /// An in-memory store (tests, and hosts that asked for no persistence).
    pub fn new(version: impl Into<String>) -> Arc<Self> {
        Self::open(version, None)
    }

    /// A store whose counters continue from the previous run.
    pub fn open(version: impl Into<String>, counters_path: Option<PathBuf>) -> Arc<Self> {
        let counters = counters_path
            .as_deref()
            .map(persist::read)
            .unwrap_or_default();
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Arc::new(Self {
            version: version.into(),
            inner: Mutex::new(Inner {
                agents: HashMap::new(),
                revision: counters.revision,
                seq: counters.state_change_seq,
                next_pane: 1,
                counters_path,
            }),
            tx,
        })
    }

    /// Next pane id in the single default workspace (`w1`).
    pub async fn allocate_pane_id(&self) -> String {
        let mut inner = self.inner.lock().await;
        let id = format!("w1:p{}", inner.next_pane);
        inner.next_pane += 1;
        id
    }

    /// Register a newly supervised agent as `unknown` (herdr starts panes
    /// unknown until detection reports otherwise).
    pub async fn add_agent(&self, agent: NewAgent) {
        let mut inner = self.inner.lock().await;
        inner.seq += 1;
        inner.revision += 1;
        let seq = inner.seq;
        let revision = inner.revision;
        let pane_id = agent.pane_id.clone();
        let label = agent.agent.clone();
        inner.agents.insert(
            agent.pane_id.clone(),
            AgentRecord {
                pane_id: agent.pane_id,
                workspace_id: agent.workspace_id,
                project: agent.project,
                agent: agent.agent,
                cwd: agent.cwd,
                label: agent.label,
                terminal_title: None,
                status: Status::Unknown,
                seen: true,
                state_change_seq: seq,
                can_resume: false,
                focused: false,
                restored: agent.restored,
            },
        );
        inner.persist();
        let _ = self.tx.send(Arc::new(Frame {
            event: EVENT_CREATED.to_string(),
            body: serde_json::json!({
                "event": EVENT_CREATED,
                "revision": revision,
                "pane_id": pane_id,
                "agent": label,
                "state_change_seq": seq,
            }),
        }));
    }

    /// Update the detected screen state. Returns true when it changed.
    pub async fn set_status(&self, pane_id: &str, status: Status) -> bool {
        let mut inner = self.inner.lock().await;
        let effective = {
            let Some(record) = inner.agents.get_mut(pane_id) else {
                return false;
            };
            if record.status == status {
                return false;
            }
            let previous = record.status;
            record.status = status;
            if status != Status::Idle {
                record.seen = true;
            } else if matches!(previous, Status::Working | Status::Blocked) {
                // Completion transition: the operator has not seen the result.
                record.seen = false;
            }
            record.effective_status()
        };
        // `state_change_seq` is global, as in herdr: one counter per host
        // process, bumped on every real status transition.
        inner.seq += 1;
        let seq = inner.seq;
        inner.revision += 1;
        let revision = inner.revision;
        let (pane_id, workspace_id, agent) = {
            let record = inner.agents.get_mut(pane_id).expect("agent exists");
            record.state_change_seq = seq;
            (
                record.pane_id.clone(),
                record.workspace_id.clone(),
                record.agent.clone(),
            )
        };
        inner.persist();
        let body = serde_json::json!({
            "event": EVENT_STATUS,
            "revision": revision,
            "pane_id": pane_id,
            "workspace_id": workspace_id,
            "agent": agent,
            "agent_status": effective,
            "state_change_seq": seq,
        });
        let _ = self.tx.send(Arc::new(Frame {
            event: EVENT_STATUS.to_string(),
            body,
        }));
        true
    }

    /// Replace the terminal's own title (OSC 0/2, or a hook's message). The
    /// row shows the operator's label first, when there is one.
    pub async fn set_terminal_title(&self, pane_id: &str, title: &str) {
        let mut inner = self.inner.lock().await;
        let Some(record) = inner.agents.get_mut(pane_id) else {
            return;
        };
        if record.terminal_title.as_deref() == Some(title) || title.is_empty() {
            return;
        }
        record.terminal_title = Some(title.to_string());
        inner.revision += 1;
        let revision = inner.revision;
        inner.persist();
        let _ = self.tx.send(Arc::new(Frame {
            event: EVENT_UPDATED.to_string(),
            body: serde_json::json!({
                "event": EVENT_UPDATED,
                "revision": revision,
                "pane_id": pane_id,
                "title": title,
            }),
        }));
    }

    /// Mark the pane as seen by the operator; a `done` row becomes `idle`.
    pub async fn mark_seen(&self, pane_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let updated = {
            let Some(record) = inner.agents.get_mut(pane_id) else {
                return false;
            };
            if record.seen {
                return true;
            }
            let before = record.effective_status();
            record.seen = true;
            let after = record.effective_status();
            if before == after {
                None
            } else {
                Some((
                    after,
                    record.state_change_seq,
                    record.pane_id.clone(),
                    record.workspace_id.clone(),
                    record.agent.clone(),
                ))
            }
        };
        if let Some((after, seq, pane_id, workspace_id, agent)) = updated {
            inner.revision += 1;
            let revision = inner.revision;
            inner.persist();
            let body = serde_json::json!({
                "event": EVENT_STATUS,
                "revision": revision,
                "pane_id": pane_id,
                "workspace_id": workspace_id,
                "agent": agent,
                "agent_status": after,
                "state_change_seq": seq,
            });
            let _ = self.tx.send(Arc::new(Frame {
                event: EVENT_STATUS.to_string(),
                body,
            }));
        }
        true
    }

    /// Forget a pane whose process is gone: a dead pane must not appear in
    /// the state list, and actions on it are refused with the contract's
    /// typed error.
    pub async fn remove_agent(&self, pane_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.agents.remove(pane_id).is_none() {
            return false;
        }
        inner.revision += 1;
        let revision = inner.revision;
        inner.persist();
        let _ = self.tx.send(Arc::new(Frame {
            event: EVENT_CLOSED.to_string(),
            body: serde_json::json!({
                "event": EVENT_CLOSED,
                "revision": revision,
                "pane_id": pane_id,
            }),
        }));
        true
    }

    pub async fn revision(&self) -> u64 {
        self.inner.lock().await.revision
    }

    /// The effective status of one pane, if it exists.
    pub async fn status_of(&self, pane_id: &str) -> Option<Status> {
        self.inner
            .lock()
            .await
            .agents
            .get(pane_id)
            .map(AgentRecord::effective_status)
    }

    /// One row, for action responses.
    pub async fn row_of(&self, pane_id: &str) -> Option<TriageRow> {
        self.inner
            .lock()
            .await
            .agents
            .get(pane_id)
            .map(AgentRecord::to_row)
    }

    pub async fn agent_exists(&self, pane_id: &str) -> bool {
        self.inner.lock().await.agents.contains_key(pane_id)
    }

    /// The full reduced state, ranked per the contract.
    pub async fn payload(&self) -> StatePayload {
        let inner = self.inner.lock().await;
        let mut rows: Vec<TriageRow> = inner.agents.values().map(AgentRecord::to_row).collect();
        rank(&mut rows);
        StatePayload {
            revision: inner.revision,
            herdr_version: self.version.clone(),
            focused: None,
            counts: counts(&rows),
            agents: rows,
        }
    }

    /// Snapshot plus a receiver for later frames. Subscribers join with the
    /// full state, so a change queued while they were connecting cannot be
    /// missed (contract `state.md`).
    pub async fn subscribe(&self) -> (StatePayload, broadcast::Receiver<SharedFrame>) {
        // Take the receiver before reading the payload: frames emitted
        // between the two calls stay queued for this subscriber.
        let rx = self.tx.subscribe();
        let payload = self.payload().await;
        (payload, rx)
    }

    pub fn receiver(&self) -> broadcast::Receiver<SharedFrame> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests;
