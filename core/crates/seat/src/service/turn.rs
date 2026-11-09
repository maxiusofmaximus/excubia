use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hn_core_chat::{new_ulid, HookScope, RunState, Turn};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

use crate::launch;
use crate::launch::LaunchInput;
use crate::output_tail::OutputTail;
use crate::profile::SelectedProfile;
use crate::prompt;
use crate::quota::looks_like_quota_exhausted;

use super::grant::HookTokenGuard;
use super::output::{capture, settle, with_output};
use super::seat::TurnEnv;
use super::worker::Worker;
use super::{lock, Phase, RunGrant, CONTEXT_CHARS, CONTEXT_MESSAGES};

pub(super) async fn run_turn(
    worker: &Worker,
    selected: &SelectedProfile,
    turn: &Turn,
) -> Result<(), String> {
    let context = worker
        .chat
        .context(&turn.conversation_id, CONTEXT_MESSAGES, CONTEXT_CHARS)
        .map_err(|error| format!("context_failed: {error}"))?;
    let boundary = format!("HN-{}", new_ulid());
    let prompt = prompt::build_turn_prompt(&context, &boundary);
    let attachment_dirs = prompt::attachment_dirs(&context);
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
    // A stop that landed before the grant existed already closed the run: no CLI
    // starts for it. From here on a stop finds the grant and wakes the wait.
    let state = worker.chat.run_state(&turn.id);
    if !matches!(state, Ok(Some(RunState::Running))) {
        lock(&worker.tokens).remove(&token);
        return state
            .map(|_| ())
            .map_err(|error| format!("run_state_failed: {error}"));
    }

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
        plugin_dir: worker.plugin_dir.as_deref(),
        attachment_dirs: &attachment_dirs,
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
    command.args(&plan.args).envs(
        plan.env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    let turn_env = (worker.turn_env)();
    command.envs(
        turn_env
            .set
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    for key in &turn_env.remove {
        command.env_remove(key);
    }
    command
        .env("HN_TOOL_TOKEN", &token)
        .current_dir(&plan.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
    // Both pipes are drained for the whole turn, so the CLI never blocks on a
    // full pipe; the tail is only read when the turn fails.
    let tail = Arc::new(Mutex::new(OutputTail::default()));
    let readers = [
        child
            .stdout
            .take()
            .map(|out| capture(out, Arc::clone(&tail))),
        child
            .stderr
            .take()
            .map(|err| capture(err, Arc::clone(&tail))),
    ];

    let outcome = wait_for_turn(&grant, &mut child, worker.timeout).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    lock(&worker.tokens).remove(&token);
    let _ = std::fs::remove_dir_all(&run_dir);

    let replied = {
        let mut phase = lock(&grant.phase);
        let replied = *phase == Phase::Replied;
        *phase = Phase::Closed;
        replied
    };
    // A stopped turn is not a failure: the store already marked it `interrupted`.
    if replied || matches!(outcome, Outcome::Stopped) {
        return Ok(());
    }
    settle(readers).await;
    let secrets = [token.as_str(), hook_token.as_str(), turn.text.as_str()];
    let excerpt = lock(&tail).excerpt(&secrets);
    // A turn on the local model is not Claude's quota: only Claude's failures are reported.
    if turn_env == TurnEnv::default() && looks_like_quota_exhausted(&excerpt) {
        (worker.on_quota_exhausted)();
    }
    Err(with_output(outcome.failure_reason(), &excerpt))
}

enum Outcome {
    Replied,
    Stopped,
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
            Self::Stopped => "stopped_by_user".to_string(),
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
    let stopped = grant.stop.notified();
    tokio::pin!(stopped);
    tokio::select! {
        _ = &mut notified => Outcome::Replied,
        _ = &mut stopped => Outcome::Stopped,
        status = child.wait() => match status {
            Ok(status) => Outcome::Exited(status.code()),
            Err(error) => Outcome::WaitError(error.to_string()),
        },
        _ = tokio::time::sleep(timeout) => Outcome::Timeout,
    }
}
