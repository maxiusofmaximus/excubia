//! Agent host for the Excubia core.
//!
//! Supervises CLI agents: each one gets a pty ([`excubia_pty`]), a terminal
//! emulator to read its screen, and a detector built from the manifests in
//! [`excubia_detect`]. Detected states are written into the authoritative
//! store ([`excubia_state`]) and the local API ([`excubia_api`]) serves them.
//!
//! Panes survive a restart as records: the agents themselves are processes
//! and are not relaunched, but their last screen is kept so a client never
//! sees an empty list.

mod actions;
mod agents;
pub mod alert_policy;
mod alerts;
pub mod alfred_backend;
pub mod alfred_switch;
pub mod audit;
pub mod calendar_reminders;
pub(crate) mod chat_file;
pub mod cli;
pub mod craft;
pub mod doc_create;
pub mod doc_edit;
pub mod doc_tools;
mod hook_token;
pub mod local_mode;
pub mod mail_api;
pub mod mail_approval;
pub(crate) mod mail_attach_later;
pub mod mail_cli;
pub(crate) mod mail_draft_input;
pub(crate) mod mail_folders;
pub(crate) mod mail_move;
pub(crate) mod mail_move_apply;
pub(crate) mod mail_move_auto;
pub(crate) mod mail_move_pick;
pub(crate) mod mail_organize;
pub(crate) mod attachment_guard;
pub(crate) mod mail_read;
pub mod mail_routes;
pub(crate) mod mail_rules;
pub mod mail_send;
pub mod mail_shadow;
pub mod mail_sidecar;
pub mod mail_style;
pub mod mail_tools;
pub(crate) mod mail_triage;
pub mod mdns;
pub mod pairing_audit;
pub mod pairing_notice;
pub mod pake_cli;
pub mod paths;
mod restore_points;
pub mod schedule_runner;
mod session;
mod snapshot;
pub mod strata;
pub mod style_learn;
pub(crate) mod style_rule;
pub mod team;
mod team_alerts;
mod team_approvals;
mod team_calendar;
mod team_contacts;
mod team_delegations;
mod team_errors;
mod team_hire_seat;
mod team_notes;
mod team_paths;
mod team_plans;
mod team_status;
mod team_tools;
mod term_size;
pub mod tls_slots;
pub mod vault;

pub use agents::{
    configured_profiles, default_catalog, engine_listing, load_or_create as load_agents,
    ready_engines,
};
pub use alerts::spawn as spawn_alerts;
pub use hook_token::load_or_create as load_hook_token;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use excubia_detect::{identify_program, Detector, TitleTracker};
use excubia_pty::{CommandSpec, PtySession, PtySize};
use excubia_state::{NewAgent, StatePayload, StateStore};
use tokio::sync::Notify;

use restore_points::RestoreStore;
use session::{Monitor, Session, SCROLLBACK_LINES};
use snapshot::SnapshotPane;
use term_size::TermSize;

/// How long a hook report stays authoritative over screen patterns.
pub const DEFAULT_HOOK_TTL: Duration = Duration::from_secs(45);
/// How many screen lines a restored pane keeps.
pub const DEFAULT_SNAPSHOT_LINES: usize = 80;
/// The cap `docs/contracts/api.md` puts on a read.
pub const MAX_SNAPSHOT_LINES: usize = 200;
/// How long to wait after the last change before writing the snapshot.
const SNAPSHOT_DEBOUNCE: Duration = Duration::from_secs(2);

/// How the host runs: where its data lives, what it tells spawned agents,
/// and the timings it uses.
pub struct HostOptions {
    pub data_dir: PathBuf,
    pub project: String,
    pub size: PtySize,
    pub hook_ttl: Duration,
    /// What spawned agents get in `EXCUBIA_HOOK_TOKEN` (empty: hooks disabled).
    pub hook_token: String,
    /// What spawned agents get in `EXCUBIA_HOOK_URL`.
    pub hook_url: String,
    pub snapshot_lines: usize,
}

impl HostOptions {
    pub fn new(data_dir: impl Into<PathBuf>, project: impl Into<String>) -> Self {
        Self {
            data_dir: data_dir.into(),
            project: project.into(),
            size: PtySize::default(),
            hook_ttl: DEFAULT_HOOK_TTL,
            hook_token: String::new(),
            hook_url: String::new(),
            snapshot_lines: DEFAULT_SNAPSHOT_LINES,
        }
    }

    pub fn snapshot_path(&self) -> PathBuf {
        paths::snapshot_path(&self.data_dir)
    }
}

/// What to run.
#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// The conversation name; becomes the pane's initial title.
    pub label: Option<String>,
    /// Extra environment, e.g. the per-seat MCP token.
    pub env: Vec<(String, String)>,
    /// The pane this seat had in the previous host run, when it has one. Its
    /// restored record is the same seat and is replaced; no other record is.
    pub replaces_pane: Option<String>,
    /// The seat this pane runs for. A restored row of the same seat is replaced
    /// even when the seat's pane id is gone (closing a pane clears it).
    pub seat_id: Option<String>,
}

impl SpawnSpec {
    /// Split a command line on whitespace: first token is the program.
    pub fn parse(command_line: &str) -> Result<Self> {
        let mut parts = command_line.split_whitespace();
        let program = parts.next().context("empty command line")?.to_string();
        Ok(Self {
            program,
            args: parts.map(str::to_string).collect(),
            cwd: None,
            label: None,
            env: Vec::new(),
            replaces_pane: None,
            seat_id: None,
        })
    }
}

/// The host. One instance supervises every agent of this process.
pub struct AgentHost {
    store: Arc<StateStore>,
    options: HostOptions,
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
    shutting_down: Arc<AtomicBool>,
    dirty: Arc<Notify>,
    hook_reports: Arc<Notify>,
    restore: Arc<RestoreStore>,
    /// Pane id → seat id, for live panes and for panes restored from a snapshot.
    seats: Arc<Mutex<HashMap<String, String>>>,
}

impl AgentHost {
    /// Build the host and restore the panes of the previous run.
    pub async fn new(store: Arc<StateStore>, options: HostOptions) -> Arc<Self> {
        let restore = Arc::new(RestoreStore::new(paths::restore_points_dir(
            &options.data_dir,
        )));
        let host = Arc::new(Self {
            store,
            options,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
            dirty: Arc::new(Notify::new()),
            hook_reports: Arc::new(Notify::new()),
            restore,
            seats: Arc::new(Mutex::new(HashMap::new())),
        });
        host.restore().await;
        host.spawn_snapshot_writer();
        host
    }

    /// The panes of the previous run: records with their last screen, marked
    /// `restored` and `unknown` (nothing is known about a process that is
    /// gone). Agents are never relaunched automatically.
    async fn restore(&self) {
        let snapshot = snapshot::read(&self.options.snapshot_path());
        if snapshot.panes.is_empty() {
            return;
        }
        let count = snapshot.panes.len();
        for pane in snapshot.panes {
            let text = pane.text();
            if let Some(seat) = &pane.seat_id {
                self.seats
                    .lock()
                    .expect("seats poisoned")
                    .insert(pane.pane_id.clone(), seat.clone());
            }
            self.store
                .add_agent(NewAgent {
                    pane_id: pane.pane_id.clone(),
                    workspace_id: pane.workspace_id,
                    project: pane.project,
                    agent: pane.agent,
                    cwd: pane.cwd,
                    label: pane.title,
                    restored: true,
                    last_active_ms: pane.last_active_ms,
                })
                .await;
            self.sessions
                .lock()
                .expect("sessions poisoned")
                .insert(pane.pane_id, Arc::new(Session::restored(text)));
        }
        eprintln!("host: restored {count} pane(s) from the previous run");
    }

    /// Drop the restored records a new live panel replaces, with their
    /// sessions: the same pane id (`add_agent` would overwrite it silently) or
    /// the pane its seat had in the previous run. Only that pane: other seats
    /// of the same project, folder and agent keep their own restored rows.
    async fn forget_replaced_restored(&self, pane_id: &str, replaces: Option<&str>, seat: Option<&str>) {
        let seats = self.seats.lock().expect("seats poisoned").clone();
        let stale: Vec<String> = self
            .store
            .payload()
            .await
            .agents
            .into_iter()
            .filter(|row| row.restored)
            .filter(|row| {
                row.pane_id == pane_id
                    || replaces == Some(row.pane_id.as_str())
                    || seat.is_some_and(|seat| seats.get(&row.pane_id).map(String::as_str) == Some(seat))
            })
            .map(|row| row.pane_id)
            .collect();
        for old in stale {
            self.store.remove_agent(&old).await;
            self.sessions
                .lock()
                .expect("sessions poisoned")
                .remove(&old);
            self.seats.lock().expect("seats poisoned").remove(&old);
        }
    }

    /// Start one agent and return its pane id.
    pub async fn start(&self, spec: SpawnSpec) -> Result<String> {
        let size = TermSize::try_from(self.options.size)?;
        let pane_id = self.store.allocate_pane_id().await;
        let kind = identify_program(&spec.program);
        let detector = kind.manifest_id().and_then(Detector::bundled).map(Arc::new);

        let cwd = spec
            .cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let mut cmd = CommandSpec::new(&spec.program);
        cmd.args = spec.args.clone();
        if let Some(cwd) = &spec.cwd {
            cmd = cmd.cwd(cwd);
        }
        // What the agent's own hooks need to report back. A spawn may override
        // them (a panel carries its own hook token), so they go first.
        cmd = cmd.env("EXCUBIA_ENV", "1").env("EXCUBIA_PANE_ID", &pane_id);
        if !self.options.hook_token.is_empty() {
            cmd = cmd.env("EXCUBIA_HOOK_TOKEN", &self.options.hook_token);
        }
        if !self.options.hook_url.is_empty() {
            cmd = cmd.env("EXCUBIA_HOOK_URL", &self.options.hook_url);
        }
        for (key, value) in &spec.env {
            cmd = cmd.env(key, value);
        }

        let mut pty = PtySession::spawn(cmd, self.options.size)
            .with_context(|| format!("spawn {:?}", spec.program))?;
        let output = pty.take_output().context("pty output")?;
        let exit = pty.take_exit().context("pty exit")?;
        let writer = pty.write_handle();
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            size.rows(),
            size.cols(),
            SCROLLBACK_LINES,
        )));
        let title = Arc::new(Mutex::new(TitleTracker::new()));

        let cwd_label = cwd.to_string_lossy().to_string();
        self.forget_replaced_restored(&pane_id, spec.replaces_pane.as_deref(), spec.seat_id.as_deref())
            .await;
        if let Some(seat) = &spec.seat_id {
            self.seats
                .lock()
                .expect("seats poisoned")
                .insert(pane_id.clone(), seat.clone());
        }
        self.store
            .add_agent(NewAgent {
                pane_id: pane_id.clone(),
                workspace_id: "w1".to_string(),
                project: self.options.project.clone(),
                agent: kind.label().to_string(),
                cwd: cwd_label,
                label: spec.label.clone(),
                restored: false,
                last_active_ms: None,
            })
            .await;

        let session = Arc::new(Session::live(pty, Arc::clone(&parser), cwd.clone()));
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .insert(pane_id.clone(), Arc::clone(&session));
        self.dirty.notify_one();

        tokio::spawn(session::monitor_agent(Monitor {
            store: Arc::clone(&self.store),
            sessions: Arc::clone(&self.sessions),
            pane_id: pane_id.clone(),
            output,
            exit,
            parser,
            title,
            detector,
            writer,
            hook_ttl: self.options.hook_ttl,
            hook_reports: Arc::clone(&self.hook_reports),
            stop: session.stop_signal(),
            dirty: Arc::clone(&self.dirty),
            shutting_down: Arc::clone(&self.shutting_down),
        }));

        Ok(pane_id)
    }

    /// Stop every agent, keeping their panes for the next run.
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Err(error) = self.save_snapshot().await {
            eprintln!("host: could not save the session snapshot: {error}");
        }
        self.kill_all_sessions();
    }

    /// Kill every live agent process now, without a snapshot. A test host does
    /// it when it is dropped, so no agent outlives its test.
    pub fn kill_all_sessions(&self) {
        let sessions: Vec<Arc<Session>> = self
            .sessions
            .lock()
            .expect("sessions poisoned")
            .values()
            .cloned()
            .collect();
        for session in sessions {
            session.kill();
        }
    }

    /// Write the current panes and their last screen to disk.
    pub async fn save_snapshot(&self) -> Result<(), String> {
        let sessions: Vec<(String, Arc<Session>)> = self
            .sessions
            .lock()
            .expect("sessions poisoned")
            .iter()
            .map(|(pane_id, session)| (pane_id.clone(), Arc::clone(session)))
            .collect();
        let mut panes = Vec::with_capacity(sessions.len());
        for (pane_id, session) in sessions {
            let Some(row) = self.store.row_of(&pane_id).await else {
                continue;
            };
            let seat_id = self.seats.lock().expect("seats poisoned").get(&pane_id).cloned();
            panes.push(SnapshotPane {
                pane_id: row.pane_id,
                workspace_id: row.workspace_id,
                project: row.project,
                agent: row.agent,
                cwd: row.cwd,
                title: row.title,
                last_active_ms: row.last_active_ms,
                seat_id,
                last_lines: session
                    .snapshot_text(self.options.snapshot_lines)
                    .lines()
                    .map(str::to_string)
                    .collect(),
            });
        }
        panes.sort_by(|a, b| a.pane_id.cmp(&b.pane_id));
        snapshot::write(&self.options.snapshot_path(), panes).map_err(|error| error.to_string())
    }

    fn spawn_snapshot_writer(self: &Arc<Self>) {
        let host = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                host.dirty.notified().await;
                tokio::time::sleep(SNAPSHOT_DEBOUNCE).await;
                if let Err(error) = host.save_snapshot().await {
                    eprintln!("host: could not save the session snapshot: {error}");
                }
            }
        });
    }

    pub(crate) fn session(&self, pane_id: &str) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .get(pane_id)
            .cloned()
    }

    /// Type one line into a live pane, as `respond` does. Used by the team
    /// dispatcher to deliver a delegation to a resident seat.
    pub fn write_line(&self, pane_id: &str, text: &str) -> Result<(), String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        let mut payload = text.as_bytes().to_vec();
        payload.push(b'\r');
        session.write(&payload)
    }

    /// The state read model `GET /v1/state` serves, for the host's own readers.
    pub async fn state_payload(&self) -> StatePayload {
        self.store.payload().await
    }

    /// How many panels are live (restored records are not processes).
    pub fn live_panes(&self) -> usize {
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .values()
            .filter(|session| session.is_live())
            .count()
    }

    /// Pane events (`pane.agent_status_changed`, `pane.closed`) for the chat
    /// card state. The API already subscribes for SSE; broadcast is multi-reader.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<excubia_state::SharedFrame> {
        self.store.receiver()
    }

    /// The child's process id, for diagnostics and the stop check.
    pub fn pid_of(&self, pane_id: &str) -> Option<u32> {
        self.session(pane_id).and_then(|session| session.pid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_splits_into_program_and_args() {
        let spec = SpawnSpec::parse("opencode --model deepseek").expect("spec");
        assert_eq!(spec.program, "opencode");
        assert_eq!(spec.args, vec!["--model", "deepseek"]);
        assert!(SpawnSpec::parse("   ").is_err());
    }

    /// A host for project `proyecto` with one restored pane already in it.
    async fn host_with_restored(pane_id: &str, project: &str, cwd: &str, agent: &str) -> Arc<AgentHost> {
        let dir = std::env::temp_dir().join(format!("excubia-restored-{}", pane_id.replace(':', "_")));
        let host = AgentHost::new(StateStore::new("test"), HostOptions::new(&dir, "proyecto")).await;
        host.store
            .add_agent(NewAgent {
                pane_id: pane_id.to_string(),
                workspace_id: "w1".to_string(),
                project: project.to_string(),
                agent: agent.to_string(),
                cwd: cwd.to_string(),
                label: None,
                restored: true,
                last_active_ms: None,
            })
            .await;
        host.sessions
            .lock()
            .expect("sessions poisoned")
            .insert(pane_id.to_string(), Arc::new(Session::restored(String::new())));
        host
    }

    /// The register step `start` runs for a new live panel, minus the spawn.
    async fn register_live(host: &AgentHost, pane_id: &str, cwd: &str, agent: &str, replaces: Option<&str>) {
        host.forget_replaced_restored(pane_id, replaces, None).await;
        host.store
            .add_agent(NewAgent {
                pane_id: pane_id.to_string(),
                workspace_id: "w1".to_string(),
                project: host.options.project.clone(),
                agent: agent.to_string(),
                cwd: cwd.to_string(),
                label: None,
                restored: false,
                last_active_ms: None,
            })
            .await;
    }

    #[tokio::test]
    async fn live_seat_replaces_its_restored_twin() {
        let host = host_with_restored("w1:p2", "proyecto", "C:/seat", "claude").await;
        register_live(&host, "w1:p1", "C:/seat", "claude", Some("w1:p2")).await;

        let rows = host.store.payload().await.agents;
        let seat: Vec<_> = rows.iter().filter(|r| r.cwd == "C:/seat" && r.agent == "claude").collect();
        assert_eq!(seat.len(), 1, "one row for the seat: {rows:?}");
        assert_eq!(seat[0].pane_id, "w1:p1");
        assert!(!seat[0].restored);
        assert!(!host.sessions.lock().unwrap().contains_key("w1:p2"));
    }

    #[tokio::test]
    async fn a_sibling_seat_restored_row_is_kept() {
        // Two seats of one project, folder and agent: the first to start must not erase the other.
        let host = host_with_restored("w1:p2", "proyecto", "C:/seat", "claude").await;
        host.store
            .add_agent(NewAgent {
                pane_id: "w1:p3".to_string(),
                workspace_id: "w1".to_string(),
                project: "proyecto".to_string(),
                agent: "claude".to_string(),
                cwd: "C:/seat".to_string(),
                label: None,
                restored: true,
                last_active_ms: None,
            })
            .await;
        host.sessions
            .lock()
            .expect("sessions poisoned")
            .insert("w1:p3".to_string(), Arc::new(Session::restored(String::new())));

        register_live(&host, "w1:p1", "C:/seat", "claude", Some("w1:p2")).await;

        let rows = host.store.payload().await.agents;
        assert!(rows.iter().any(|r| r.pane_id == "w1:p3" && r.restored), "sibling kept: {rows:?}");
        assert!(host.sessions.lock().unwrap().contains_key("w1:p3"));
        assert!(!rows.iter().any(|r| r.pane_id == "w1:p2"), "its own twin is gone: {rows:?}");
    }

    #[tokio::test]
    async fn a_start_without_its_seat_pane_removes_no_restored_row() {
        let host = host_with_restored("w1:p2", "proyecto", "C:/seat", "claude").await;
        register_live(&host, "w1:p1", "C:/seat", "claude", None).await;

        let rows = host.store.payload().await.agents;
        assert_eq!(rows.len(), 2, "both rows stay: {rows:?}");
        assert!(host.sessions.lock().unwrap().contains_key("w1:p2"));
    }

    #[tokio::test]
    async fn a_seat_on_a_new_pane_replaces_its_restored_row_without_its_old_pane() {
        // Closing the old pane cleared the seat's pane id, so only the seat link can match.
        let host = host_with_restored("w1:p2", "proyecto", "C:/seat", "claude").await;
        host.seats.lock().unwrap().insert("w1:p2".to_string(), "ag_seat".to_string());
        host.forget_replaced_restored("w1:p1", None, Some("ag_other")).await;
        assert!(host.store.payload().await.agents.iter().any(|r| r.pane_id == "w1:p2"), "other seat keeps it");

        host.forget_replaced_restored("w1:p1", None, Some("ag_seat")).await;
        let rows = host.store.payload().await.agents;
        assert!(!rows.iter().any(|r| r.pane_id == "w1:p2"), "its own restored row is gone: {rows:?}");
        assert!(!host.seats.lock().unwrap().contains_key("w1:p2"));
    }
}
