//! CORE7 team surface: projects, seat panels, delegations, cards and the
//! single coalesced Alfred synthesis.
//!
//! The API talks to this module only through `hn_core_chat::ChatSeat`; the
//! CLI processes, PTYs and pane events live here and in [`crate::AgentHost`].
//! Real resident CLIs are `[NV]`; the simulated CLI exercises the same launch
//! materialisation, per-seat token and MCP transport.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hn_core_chat::{
    ChatSeat, ChatStore, SeatActor, SeatError, ToolError, ToolReply, ToolRequest, Turn,
};
use hn_core_seat::{parse_tool, SeatService};
use serde_json::json;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::AgentHost;

/// `orchestrator-chat.md` §5: six live panels per host.
pub const MAX_LIVE_PANES: usize = 6;
/// A delegation that has not completed in ten minutes is failed.
pub const DELEGATION_TIMEOUT: Duration = Duration::from_secs(600);
/// At most this many pending results are summarised in one Alfred turn.
pub(super) const SYNTHESIS_BATCH: usize = 8;

pub struct TeamConfig {
    pub chat: Arc<ChatStore>,
    pub host: Arc<AgentHost>,
    pub alfred: Arc<SeatService>,
    pub data_dir: PathBuf,
    pub workspaces_root: PathBuf,
    pub delegation_timeout: Duration,
    /// Where the OpenCode UAC plugin lives. Without it there is no blocking
    /// adapter to verify and the seats stay `unchecked`.
    pub uac_plugin_dir: Option<PathBuf>,
}

pub struct TeamService {
    pub(super) chat: Arc<ChatStore>,
    pub(super) host: Arc<AgentHost>,
    pub(super) alfred: Arc<SeatService>,
    pub(super) data_dir: PathBuf,
    pub(super) workspaces_root: PathBuf,
    pub(super) delegation_timeout: Duration,
    pub(super) uac_plugin_dir: Option<PathBuf>,
    pub(super) grants: Mutex<HashMap<String, TeamGrant>>,
    pub(super) watchers: Mutex<HashMap<String, Watcher>>,
    pub(super) synthesis: Arc<Notify>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone, Debug)]
pub(super) struct TeamGrant {
    pub(super) actor: SeatActor,
    pub(super) pane_id: String,
}

pub(super) struct Watcher {
    pub(super) stop: tokio::sync::oneshot::Sender<()>,
    pub(super) task: JoinHandle<()>,
}

impl TeamService {
    /// Validate the workspace root, then start the synthesis worker and the
    /// pane-state listener. A host with no resident-capable CLI still starts:
    /// every spawn answers a typed `seat_unavailable`.
    pub fn start(config: TeamConfig) -> Result<Arc<Self>, SeatError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            SeatError::unavailable("the team seat needs a Tokio runtime to start its workers")
        })?;
        std::fs::create_dir_all(&config.workspaces_root)
            .map_err(|error| SeatError::unavailable(format!("workspaces root: {error}")))?;
        let workspaces_root = config
            .workspaces_root
            .canonicalize()
            .map_err(|error| SeatError::unavailable(format!("workspaces root: {error}")))?;
        config.chat.set_workspaces_root(&workspaces_root);
        let service = Arc::new(Self {
            chat: Arc::clone(&config.chat),
            host: Arc::clone(&config.host),
            alfred: Arc::clone(&config.alfred),
            data_dir: config.data_dir,
            workspaces_root,
            delegation_timeout: config.delegation_timeout,
            uac_plugin_dir: config.uac_plugin_dir,
            grants: Mutex::new(HashMap::new()),
            watchers: Mutex::new(HashMap::new()),
            synthesis: Arc::new(Notify::new()),
            tasks: Mutex::new(Vec::new()),
        });

        let synthesis = Arc::clone(&service);
        let synthesis_task = runtime.spawn(async move { synthesis.synthesis_loop().await });
        let listener = Arc::clone(&service);
        let pane_task = runtime.spawn(async move { listener.listen_to_panes().await });
        lock(&service.tasks).extend([synthesis_task, pane_task]);
        Ok(service)
    }

    pub(super) fn grant(&self, token: &str) -> Option<TeamGrant> {
        lock(&self.grants).get(token).cloned()
    }

    /// Test seam: give `token` the grant that a panel start gives its seat, so
    /// a test can call the tool surface with the token of a real seat.
    #[doc(hidden)]
    pub fn grant_for_tests(&self, token: &str, actor: SeatActor, pane_id: &str) {
        lock(&self.grants).insert(
            token.to_string(),
            TeamGrant {
                actor,
                pane_id: pane_id.to_string(),
            },
        );
    }

    pub(super) fn remove_grant_for_pane(&self, pane_id: &str) {
        lock(&self.grants).retain(|_, grant| grant.pane_id != pane_id);
    }
}

#[async_trait::async_trait]
impl ChatSeat for TeamService {
    fn try_enqueue(&self, turn: Turn) -> Result<(), SeatError> {
        self.alfred.try_enqueue(turn)
    }

    fn actor(&self, token: &str) -> Option<SeatActor> {
        self.grant(token)
            .map(|grant| grant.actor)
            .or_else(|| self.alfred.actor(token))
    }

    async fn tool(&self, token: &str, request: ToolRequest) -> Result<ToolReply, ToolError> {
        if let Some(grant) = self.grant(token) {
            let call = parse_tool(&request)?;
            return self.team_tool(grant, call).await;
        }
        let Some(actor) = self.alfred.actor(token) else {
            return Err(ToolError::unauthorized("unknown or expired tool token"));
        };
        let call = parse_tool(&request)?;
        self.alfred_tool(actor, call, token, request).await
    }

    fn cancel_delegation(&self, delegation_id: &str) -> Result<(), SeatError> {
        self.stop_watcher(delegation_id);
        if let Ok(delegation) = self.chat.delegation(delegation_id) {
            if let Some(pane_id) = delegation.pane_id.as_deref() {
                let payload = json!({ "cancel": delegation_id, "reason": "user_cancelled" });
                let _ = self.host.write_line(pane_id, &payload.to_string());
            }
        }
        Ok(())
    }

    fn cancel_conversation(&self, conversation_id: &str) -> Result<(), SeatError> {
        let ids = self
            .chat
            .active_delegations_for_conversation(conversation_id)
            .map_err(|error| SeatError::unavailable(error.to_string()))?;
        for id in ids {
            self.cancel_delegation(&id)?;
        }
        Ok(())
    }

    fn shutdown(&self) {
        self.alfred.shutdown();
        for (_, watcher) in lock(&self.watchers).drain() {
            let _ = watcher.stop.send(());
            watcher.task.abort();
        }
        for task in lock(&self.tasks).drain(..) {
            task.abort();
        }
        lock(&self.grants).clear();
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
