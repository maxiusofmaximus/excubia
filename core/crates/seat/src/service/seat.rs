use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use excubia_chat::{
    AttachmentError, ChatError, ChatSeat, ChatStore, SeatActor, SeatError, ToolError,
    ToolErrorKind, ToolReply, ToolRequest, Turn, MAX_ATTACHMENTS_PER_MESSAGE,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::launch;
use crate::profile::{self, CliProfile, SelectedProfile};
use crate::prompt;
use crate::tools::{self, ToolCall};

use super::worker::{run_worker, Worker};
use super::RunGrant;

pub const CONTEXT_MESSAGES: usize = 20;
pub const CONTEXT_CHARS: usize = 24_000;

/// Environment changes for one turn: set these variables, drop those.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnEnv {
    pub set: Vec<(String, String)>,
    pub remove: Vec<String>,
}

pub struct SeatConfig {
    pub data_dir: PathBuf,
    pub chat: Arc<ChatStore>,
    pub mcp_command: PathBuf,
    pub tool_url: String,
    /// The loopback address of the hook surface the agent's own hook calls.
    pub hook_url: String,
    /// The `excubia-uac-hook` binary, wired as the Claude blocking pre-tool hook.
    pub hook_command: PathBuf,
    /// The OpenCode UAC plugin directory, when the host has one. Without it the
    /// OpenCode seat runs with no plugin.
    pub plugin_dir: Option<PathBuf>,
    pub profiles: Vec<CliProfile>,
    /// CLI engines Alfred may choose for its turns, ready at start (canary and version).
    pub engines: Vec<SelectedProfile>,
    /// Read at the start of each turn: the name of the engine that answers it, if any.
    pub turn_engine: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub queue_capacity: usize,
    pub turn_timeout: Duration,
    /// Read at the start of each turn: the CLI's environment changes, e.g. the local model.
    pub turn_env: Arc<dyn Fn() -> TurnEnv + Send + Sync>,
    /// Called when a turn with no environment change fails with a quota message. Only a
    /// signal: the turn still fails.
    pub on_quota_exhausted: Arc<dyn Fn() + Send + Sync>,
}

pub struct SeatService {
    pub(crate) chat: Arc<ChatStore>,
    agent_id: String,
    selected: Option<SelectedProfile>,
    has_engines: bool,
    mcp_command: PathBuf,
    tool_url: String,
    sender: mpsc::Sender<Turn>,
    tokens: Arc<Mutex<HashMap<String, Arc<RunGrant>>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    closed: AtomicBool,
}

impl SeatService {
    /// Seed the role, probe the matrix and start the worker. A host with no
    /// installed CLI still starts: every turn gets `seat_unavailable`.
    pub fn start(config: SeatConfig) -> Result<Arc<Self>, SeatError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            SeatError::unavailable("the seat needs a Tokio runtime to start its worker")
        })?;
        if !launch::hook_ready(&config.hook_command) {
            return Err(SeatError::unavailable(format!(
                "the approval hook is missing or not runnable: {}",
                config.hook_command.display()
            )));
        }
        let role = prompt::seed_and_load(&config.data_dir)
            .map_err(|error| SeatError::unavailable(format!("role prompt: {error}")))?;
        let seat_dir = config.data_dir.join("seats").join("alfred");
        std::fs::create_dir_all(&seat_dir)
            .map_err(|error| SeatError::unavailable(format!("seat directory: {error}")))?;
        let agent_id = config
            .chat
            .alfred_id()
            .map_err(|error| SeatError::unavailable(format!("Alfred's seat: {error}")))?;
        config
            .chat
            .set_seat_prompt_hash(&agent_id, &role.hash)
            .map_err(|error| SeatError::unavailable(format!("seat prompt hash: {error}")))?;

        let selected = if config.mcp_command.is_file() {
            profile::select(&config.profiles, profile::probe)
        } else {
            None
        };
        match &selected {
            Some(selected) => {
                config
                    .chat
                    .set_seat_adapter(&agent_id, selected.profile.id.as_str())
                    .map_err(|error| SeatError::unavailable(format!("seat adapter: {error}")))?;
                config
                    .chat
                    .set_seat_status(&agent_id, "idle")
                    .map_err(|error| SeatError::unavailable(format!("seat status: {error}")))?;
            }
            None => {
                config
                    .chat
                    .set_seat_status(&agent_id, "stopped")
                    .map_err(|error| SeatError::unavailable(format!("seat status: {error}")))?;
            }
        }

        let (sender, receiver) = mpsc::channel(config.queue_capacity.max(1));
        let tokens = Arc::new(Mutex::new(HashMap::new()));
        let service = Arc::new(Self {
            chat: Arc::clone(&config.chat),
            agent_id: agent_id.clone(),
            selected: selected.clone(),
            has_engines: !config.engines.is_empty(),
            mcp_command: config.mcp_command.clone(),
            tool_url: config.tool_url.clone(),
            sender,
            tokens: Arc::clone(&tokens),
            worker: Mutex::new(None),
            closed: AtomicBool::new(false),
        });

        let worker = runtime.spawn(run_worker(Worker {
            chat: config.chat,
            receiver,
            selected,
            engines: config.engines,
            turn_engine: config.turn_engine,
            tokens,
            mcp_command: config.mcp_command,
            tool_url: config.tool_url,
            hook_url: config.hook_url,
            hook_command: config.hook_command,
            plugin_dir: config.plugin_dir,
            seat_dir,
            role,
            timeout: config.turn_timeout,
            agent_id,
            turn_env: config.turn_env,
            on_quota_exhausted: config.on_quota_exhausted,
        }));
        *lock(&service.worker) = Some(worker);
        Ok(service)
    }

    pub fn selected(&self) -> Option<&SelectedProfile> {
        self.selected.as_ref()
    }

    pub fn mcp_command(&self) -> &std::path::Path {
        &self.mcp_command
    }

    pub fn tool_url(&self) -> &str {
        &self.tool_url
    }

    pub fn is_available(&self) -> bool {
        (self.selected.is_some() || self.has_engines) && !self.closed.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ChatSeat for SeatService {
    fn try_enqueue(&self, turn: Turn) -> Result<(), SeatError> {
        if !self.is_available() {
            let _ = self.chat.fail_run(&turn.id, "seat_unavailable");
            return Err(SeatError::unavailable(
                "no installed CLI matches the seat matrix",
            ));
        }
        match self.sender.try_send(turn) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(turn)) => {
                let _ = self.chat.fail_run(&turn.id, "agent_busy");
                Err(SeatError::busy("the seat queue is full"))
            }
            Err(mpsc::error::TrySendError::Closed(turn)) => {
                let _ = self.chat.fail_run(&turn.id, "seat_unavailable");
                Err(SeatError::unavailable("the seat worker is stopped"))
            }
        }
    }

    fn actor(&self, token: &str) -> Option<SeatActor> {
        lock(&self.tokens)
            .get(token)
            .map(|grant| grant.actor.clone())
    }

    async fn tool(&self, token: &str, request: ToolRequest) -> Result<ToolReply, ToolError> {
        let grant = lock(&self.tokens)
            .get(token)
            .cloned()
            .ok_or_else(|| ToolError::unauthorized("unknown or expired tool token"))?;
        let call = tools::parse(&request)?;
        match call {
            ToolCall::Reply {
                text,
                suggestions,
                attachment_ids,
            } => self.reply(&grant, text, &suggestions, &attachment_ids),
            ToolCall::AskUser(question) => self.ask(&grant, question),
            ToolCall::Progress(text) => self.progress(&grant, &text),
            ToolCall::ApprovalRequest(request) => self.ask_approval(&grant, request).await,
            _ => Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "this tool is handled by the team seat, not by the chat turn",
            )),
        }
    }

    fn stop_turns(&self, conversation_id: &str) {
        for grant in lock(&self.tokens).values() {
            if grant.actor.conversation_id == conversation_id {
                grant.stop.notify_one();
            }
        }
    }

    fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(worker) = lock(&self.worker).take() {
            worker.abort();
        }
        lock(&self.tokens).clear();
        let _ = self.chat.interrupt_active("host_shutdown");
        let _ = self.chat.set_seat_status(&self.agent_id, "stopped");
    }
}

impl Drop for SeatService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A file the reply named that cannot be shown: the agent reads the Spanish line and
/// fixes its list. Any other attachment failure stays an unavailable chat.
fn file_refusal(error: &AttachmentError) -> Option<String> {
    let line = match error {
        AttachmentError::UnknownAttachment => {
            "ese archivo no existe en esta conversación o no lo creó Alfred"
        }
        AttachmentError::AlreadyLinked => "ese archivo ya está en otro mensaje",
        AttachmentError::NotReady => "ese archivo todavía no está listo",
        AttachmentError::TooManyAttachments => {
            return Some(format!(
                "un mensaje lleva como máximo {MAX_ATTACHMENTS_PER_MESSAGE} archivos"
            ))
        }
        _ => return None,
    };
    Some(line.to_string())
}

pub(crate) fn tool_chat_error(error: ChatError) -> ToolError {
    if let ChatError::Attachment(file) = &error {
        if let Some(message) = file_refusal(file) {
            return ToolError::invalid("invalid_arguments", "attachment_ids", message);
        }
    }
    match error {
        ChatError::RunNotActive(_) | ChatError::NoSuchRun(_) => {
            ToolError::conflict("run_closed", "the run is no longer active")
        }
        other => ToolError {
            kind: ToolErrorKind::Unavailable,
            code: "chat_unavailable",
            field: None,
            message: other.to_string(),
        },
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
