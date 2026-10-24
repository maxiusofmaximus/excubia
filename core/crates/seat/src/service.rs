//! Alfred's seat: a one-turn FIFO worker, a per-run MCP token registry and the
//! only writer of `queued -> running -> completed|failed|interrupted`.
//!
//! The HTTP route enqueues synchronously so a full queue or a missing CLI is a
//! real `409`, not a late failure. The worker launches one isolated CLI process
//! per turn, passes the prompt on stdin and never reads stdout: the only valid
//! answer arrives through `chat.reply`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hn_core_chat::{
    new_ulid, ChatError, ChatSeat, ChatStore, HookScope, SeatActor, SeatError, ToolError,
    ToolErrorKind, ToolReply, ToolRequest, Turn,
};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::launch::{self, LaunchInput};
use crate::profile::{self, CliProfile, SelectedProfile};
use crate::prompt::{self, Role};
use crate::tools::{self, ToolCall};

pub const CONTEXT_MESSAGES: usize = 20;
pub const CONTEXT_CHARS: usize = 24_000;

pub struct SeatConfig {
    pub data_dir: PathBuf,
    pub chat: Arc<ChatStore>,
    pub mcp_command: PathBuf,
    pub tool_url: String,
    /// The loopback address of the hook surface the agent's own hook calls.
    pub hook_url: String,
    /// The `hn-uac-hook` binary, wired as the Claude blocking pre-tool hook.
    pub hook_command: PathBuf,
    pub profiles: Vec<CliProfile>,
    pub queue_capacity: usize,
    pub turn_timeout: Duration,
}

pub struct SeatService {
    pub(crate) chat: Arc<ChatStore>,
    agent_id: String,
    selected: Option<SelectedProfile>,
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
            tokens,
            mcp_command: config.mcp_command,
            tool_url: config.tool_url,
            hook_url: config.hook_url,
            hook_command: config.hook_command,
            seat_dir,
            role,
            timeout: config.turn_timeout,
            agent_id,
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
        self.selected.is_some() && !self.closed.load(Ordering::SeqCst)
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
            ToolCall::Reply(text) => self.reply(&grant, text),
            ToolCall::AskUser(question) => self.ask(&grant, question),
            ToolCall::ApprovalRequest(request) => self.ask_approval(&grant, request).await,
            _ => Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "this tool is handled by the team seat, not by the chat turn",
            )),
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

pub(crate) fn tool_chat_error(error: ChatError) -> ToolError {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Running,
    Replied,
    Closed,
}

pub(crate) struct RunGrant {
    pub(crate) run_id: String,
    pub(crate) actor: SeatActor,
    pub(crate) phase: Mutex<Phase>,
    pub(crate) notify: Notify,
}

impl RunGrant {
    fn new(turn: &Turn) -> Self {
        Self {
            run_id: turn.id.clone(),
            actor: SeatActor {
                agent_id: turn.agent_id.clone(),
                role_id: "alfred".to_string(),
                project_id: None,
                conversation_id: turn.conversation_id.clone(),
                run_id: turn.id.clone(),
            },
            phase: Mutex::new(Phase::Running),
            notify: Notify::new(),
        }
    }
}

struct Worker {
    chat: Arc<ChatStore>,
    receiver: mpsc::Receiver<Turn>,
    selected: Option<SelectedProfile>,
    tokens: Arc<Mutex<HashMap<String, Arc<RunGrant>>>>,
    mcp_command: PathBuf,
    tool_url: String,
    hook_url: String,
    hook_command: PathBuf,
    seat_dir: PathBuf,
    role: Role,
    timeout: Duration,
    agent_id: String,
}

async fn run_worker(mut worker: Worker) {
    while let Some(turn) = worker.receiver.recv().await {
        match worker.selected.clone() {
            Some(selected) => process_turn(&worker, &selected, turn).await,
            None => {
                let _ = worker.chat.fail_run(&turn.id, "seat_unavailable");
            }
        }
    }
}

async fn process_turn(worker: &Worker, selected: &SelectedProfile, turn: Turn) {
    match worker.chat.start_run(&turn.id) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            eprintln!("seat: could not start run {}: {error}", turn.id);
            return;
        }
    }
    let _ = worker.chat.set_seat_status(&worker.agent_id, "working");
    if let Err(reason) = run_turn(worker, selected, &turn).await {
        let _ = worker.chat.fail_run_with_card(&turn.id, &reason);
    }
    let _ = worker.chat.set_seat_status(&worker.agent_id, "idle");
}

async fn run_turn(worker: &Worker, selected: &SelectedProfile, turn: &Turn) -> Result<(), String> {
    let context = worker
        .chat
        .context(&turn.conversation_id, CONTEXT_MESSAGES, CONTEXT_CHARS)
        .map_err(|error| format!("context_failed: {error}"))?;
    let boundary = format!("HN-{}", new_ulid());
    let prompt = prompt::build_turn_prompt(&context, &boundary);
    let run_dir = worker.seat_dir.join("runs").join(&turn.id);

    // The hook credential of this turn: minted here, handed to the CLI in its
    // environment, revoked when the turn closes (the guard covers every exit).
    let hook_scope = HookScope::run(&turn.id, &turn.conversation_id, &worker.agent_id);
    let hook_token = worker
        .chat
        .issue_hook_token(&hook_scope)
        .map_err(|error| format!("hook_token_failed: {error}"))?;
    let _hook_token = HookTokenGuard {
        chat: &worker.chat,
        run_id: &turn.id,
    };
    let token = new_ulid();
    let grant = Arc::new(RunGrant::new(turn));
    lock(&worker.tokens).insert(token.clone(), Arc::clone(&grant));

    let input = LaunchInput {
        profile: &selected.profile,
        run_id: &turn.id,
        agent_id: &worker.agent_id,
        conversation_id: &turn.conversation_id,
        run_dir: &run_dir,
        role: &worker.role.text,
        prompt: &prompt,
        tool_url: &worker.tool_url,
        mcp_command: &worker.mcp_command,
        session_id: &worker.agent_id,
        hook_url: &worker.hook_url,
        hook_token: &hook_token,
        hook_command: &worker.hook_command,
    };
    let plan = match launch::prepare(&input) {
        Ok(plan) => plan,
        Err(error) => {
            lock(&worker.tokens).remove(&token);
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(format!("prepare_failed: {error}"));
        }
    };
    if let Err(error) = plan.materialise() {
        lock(&worker.tokens).remove(&token);
        let _ = std::fs::remove_dir_all(&run_dir);
        return Err(format!("prepare_failed: {error}"));
    }

    let mut command = Command::new(&plan.program);
    command
        .args(&plan.args)
        .envs(
            plan.env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .env("HN_TOOL_TOKEN", &token)
        .current_dir(&plan.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            lock(&worker.tokens).remove(&token);
            let _ = std::fs::remove_dir_all(&run_dir);
            return Err(format!("spawn_failed: {error}"));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = plan.stdin;
        tokio::spawn(async move {
            let _ = stdin.write_all(bytes.as_bytes()).await;
        });
    }

    let outcome = wait_for_turn(&grant, &mut child, worker.timeout).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    lock(&worker.tokens).remove(&token);
    let _ = std::fs::remove_dir_all(&run_dir);

    let mut phase = lock(&grant.phase);
    let replied = *phase == Phase::Replied;
    *phase = Phase::Closed;
    drop(phase);
    if replied {
        return Ok(());
    }
    Err(outcome.failure_reason())
}

enum Outcome {
    Replied,
    Exited(Option<i32>),
    Timeout,
    WaitError(String),
}

impl Outcome {
    /// Only reached when `chat.reply` did not commit. A notification without a
    /// committed reply is a bug, not a success: it still fails closed.
    fn failure_reason(self) -> String {
        match self {
            Self::Replied => "reply_not_committed".to_string(),
            Self::Exited(Some(code)) => format!("process_exit: {code}"),
            Self::Exited(None) => "process_exit: signal".to_string(),
            Self::Timeout => "turn_timeout".to_string(),
            Self::WaitError(error) => format!("process_wait_failed: {error}"),
        }
    }
}

async fn wait_for_turn(grant: &RunGrant, child: &mut Child, timeout: Duration) -> Outcome {
    let notified = grant.notify.notified();
    tokio::pin!(notified);
    tokio::select! {
        _ = &mut notified => Outcome::Replied,
        status = child.wait() => match status {
            Ok(status) => Outcome::Exited(status.code()),
            Err(error) => Outcome::WaitError(error.to_string()),
        },
        _ = tokio::time::sleep(timeout) => Outcome::Timeout,
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The run's hook token lives exactly as long as the turn: every exit path of
/// `run_turn` revokes it, so a stale hook can never answer for a closed run.
struct HookTokenGuard<'a> {
    chat: &'a ChatStore,
    run_id: &'a str,
}

impl Drop for HookTokenGuard<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.chat.revoke_hook_token(self.run_id) {
            eprintln!(
                "seat: could not revoke the hook token of {}: {error}",
                self.run_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_chat::HookTokenUse;

    #[test]
    fn the_guard_revokes_the_token_of_the_run() {
        let chat = ChatStore::in_memory().expect("store");
        let token = chat
            .issue_hook_token(&HookScope::run("run_1", "cnv_1", "ag_1"))
            .expect("token");
        {
            let _guard = HookTokenGuard {
                chat: &chat,
                run_id: "run_1",
            };
        }
        assert_eq!(
            chat.hook_token_use(&token).expect("use"),
            HookTokenUse::Revoked
        );
    }
}
