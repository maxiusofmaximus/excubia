//! Agent host for the herdr-notify core.
//!
//! Supervises CLI agents: each one gets a pty ([`hn_core_pty`]), a terminal
//! emulator to read its screen, and a detector built from the manifests in
//! [`hn_core_detect`]. Detected states are written into the authoritative
//! store ([`hn_core_state`]) and the local API ([`hn_core_api`]) serves them.
//!
//! Panes survive a restart as records: the agents themselves are processes
//! and are not relaunched, but their last screen is kept so a client never
//! sees an empty list.

mod actions;
mod agents;
pub mod cli;
mod hook_token;
pub mod paths;
mod session;
mod snapshot;
pub mod team;
mod team_approvals;
mod team_delegations;
mod team_errors;
mod team_paths;
mod team_tools;
pub mod tls_slots;

pub use agents::{default_catalog, load_or_create as load_agents};
pub use hook_token::load_or_create as load_hook_token;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use hn_core_detect::{identify_program, Detector, TitleTracker};
use hn_core_pty::{CommandSpec, PtySession, PtySize};
use hn_core_state::{NewAgent, StateStore};
use tokio::sync::Notify;

use session::{Monitor, Session, SCROLLBACK_LINES};
use snapshot::SnapshotPane;

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
    /// What spawned agents get in `HN_HOOK_TOKEN` (empty: hooks disabled).
    pub hook_token: String,
    /// What spawned agents get in `HN_HOOK_URL`.
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
}

impl AgentHost {
    /// Build the host and restore the panes of the previous run.
    pub async fn new(store: Arc<StateStore>, options: HostOptions) -> Arc<Self> {
        let host = Arc::new(Self {
            store,
            options,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            shutting_down: Arc::new(AtomicBool::new(false)),
            dirty: Arc::new(Notify::new()),
            hook_reports: Arc::new(Notify::new()),
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
            self.store
                .add_agent(NewAgent {
                    pane_id: pane.pane_id.clone(),
                    workspace_id: pane.workspace_id,
                    project: pane.project,
                    agent: pane.agent,
                    cwd: pane.cwd,
                    label: pane.title,
                    restored: true,
                })
                .await;
            self.sessions
                .lock()
                .expect("sessions poisoned")
                .insert(pane.pane_id, Arc::new(Session::restored(text)));
        }
        eprintln!("host: restored {count} pane(s) from the previous run");
    }

    /// Start one agent and return its pane id.
    pub async fn start(&self, spec: SpawnSpec) -> Result<String> {
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
        cmd = cmd.env("HN_ENV", "1").env("HN_PANE_ID", &pane_id);
        if !self.options.hook_token.is_empty() {
            cmd = cmd.env("HN_HOOK_TOKEN", &self.options.hook_token);
        }
        if !self.options.hook_url.is_empty() {
            cmd = cmd.env("HN_HOOK_URL", &self.options.hook_url);
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
            self.options.size.rows,
            self.options.size.cols,
            SCROLLBACK_LINES,
        )));
        let title = Arc::new(Mutex::new(TitleTracker::new()));

        let cwd_label = cwd.to_string_lossy().to_string();
        self.store
            .add_agent(NewAgent {
                pane_id: pane_id.clone(),
                workspace_id: "w1".to_string(),
                project: self.options.project.clone(),
                agent: kind.label().to_string(),
                cwd: cwd_label,
                label: spec.label.clone(),
                restored: false,
            })
            .await;

        let session = Arc::new(Session::live(pty, Arc::clone(&parser)));
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
            panes.push(SnapshotPane {
                pane_id: row.pane_id,
                workspace_id: row.workspace_id,
                project: row.project,
                agent: row.agent,
                cwd: row.cwd,
                title: row.title,
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
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<hn_core_state::SharedFrame> {
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
}
