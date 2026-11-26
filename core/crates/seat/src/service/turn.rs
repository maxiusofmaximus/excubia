use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use excubia_chat::{
    is_wait_frame, new_ulid, until, ActiveBudget, ApprovalWaits, ChatStore, HookScope, RunState,
    Turn,
};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::broadcast::error::RecvError;

use crate::free_models::{is_free_limit, FreeModels, EXHAUSTED_REASON};
use crate::launch;
use crate::launch::LaunchInput;
use crate::output_tail::OutputTail;
use crate::profile::{CliId, SelectedProfile};
use crate::prompt;
use crate::quota::looks_like_quota_exhausted;

use super::grant::HookTokenGuard;
use super::output::{capture, settle, with_output};
use super::seat::TurnEnv;
use super::worker::Worker;
use super::{lock, Phase, RunGrant, CONTEXT_CHARS, CONTEXT_MESSAGES};

/// Runs the CLI for a turn that answers `run_ids` (its first run is `turn.id`). When the
/// turn merges several messages, `turn.text` holds them in order. An OpenCode turn runs on
/// the free models not known to be spent, in order: when one reports its free tier spent, the
/// turn retries on the next, and fails with `free_models_exhausted` when none is left.
pub(super) async fn run_turn(
    worker: &Worker,
    selected: &SelectedProfile,
    turn: &Turn,
    run_ids: &[String],
) -> Result<(), String> {
    let context = worker
        .chat
        .context(&turn.conversation_id, CONTEXT_MESSAGES, CONTEXT_CHARS)
        .map_err(|error| format!("context_failed: {error}"))?;
    let boundary = format!("EXCUBIA-{}", new_ulid());
    // A host turn names no message: its text is the instruction the CLI answers.
    let current = if run_ids.len() > 1 {
        prompt::Current::Merged(&turn.text)
    } else if turn.message_id.is_empty() {
        prompt::Current::Host(&turn.text)
    } else {
        prompt::Current::LastUser
    };
    let prompt = prompt::build_turn_prompt(&context, &boundary, current);
    let attachment_dirs = prompt::attachment_dirs(&context);

    let attempts: Vec<Option<&'static str>> = if selected.profile.id == CliId::OpenCode {
        worker
            .free_models
            .usable(Instant::now())
            .into_iter()
            .map(Some)
            .collect()
    } else {
        vec![None]
    };
    let (prompt, attachment_dirs) = (prompt.as_str(), attachment_dirs.as_slice());
    answer_on_engines(&worker.free_models, attempts, |engine_model| {
        attempt(
            worker,
            selected,
            turn,
            run_ids,
            prompt,
            attachment_dirs,
            engine_model,
        )
    })
    .await
}

/// How one CLI run of a turn ended, when it did not fail outright.
enum Attempt {
    /// The turn answered, or the user stopped it.
    Done,
    /// The free model's tier is spent; the excerpt is the CLI's last output.
    FreeLimit(String),
    /// The free model's run ended without `chat.reply` and without a stop: the turn moves on
    /// to the next model. The string is the failure the run would have reported.
    NoAnswer(String),
}

/// Runs the turn on each engine in order until one answers. A spent free model is remembered
/// in `book` and the next one is tried; a model that ends without answering is only skipped for
/// this turn. The last model's failure is the turn's failure: `free_models_exhausted` when it
/// reported its tier spent, its own failure otherwise.
async fn answer_on_engines<Fut>(
    book: &FreeModels,
    engines: Vec<Option<&'static str>>,
    mut run: impl FnMut(Option<&'static str>) -> Fut,
) -> Result<(), String>
where
    Fut: Future<Output = Result<Attempt, String>>,
{
    let mut excerpt = String::new();
    let mut last_failure = None;
    for engine_model in engines {
        last_failure = None;
        match run(engine_model).await? {
            Attempt::Done => return Ok(()),
            Attempt::FreeLimit(output) => {
                if let Some(model) = engine_model {
                    eprintln!("seat: {model} has no free usage left");
                    book.mark_exhausted(model, Instant::now());
                }
                excerpt = output;
            }
            Attempt::NoAnswer(failure) => {
                if let Some(model) = engine_model {
                    eprintln!("seat: {model} ended without answering; trying the next model");
                }
                last_failure = Some(failure);
            }
        }
    }
    Err(last_failure.unwrap_or_else(|| with_output(EXHAUSTED_REASON.to_string(), &excerpt)))
}

/// One CLI run of the turn. `engine_model` is the free OpenCode model it runs on, `None` for
/// any other CLI.
async fn attempt(
    worker: &Worker,
    selected: &SelectedProfile,
    turn: &Turn,
    run_ids: &[String],
    prompt: &str,
    attachment_dirs: &[PathBuf],
    engine_model: Option<&str>,
) -> Result<Attempt, String> {
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
    let grant = Arc::new(RunGrant::new(turn, run_ids.to_vec()));
    lock(&worker.tokens).insert(token.clone(), Arc::clone(&grant));
    // A stop that landed before the grant existed already closed the run: no CLI
    // starts for it. From here on a stop finds the grant and wakes the wait.
    let state = worker.chat.run_state(&turn.id);
    if !matches!(state, Ok(Some(RunState::Running))) {
        lock(&worker.tokens).remove(&token);
        return state
            .map(|_| Attempt::Done)
            .map_err(|error| format!("run_state_failed: {error}"));
    }

    // The reply this run writes names the free model it runs on (`message.model`).
    if let Some(model) = engine_model {
        if let Err(error) = worker.chat.set_run_model(&turn.id, model) {
            eprintln!("seat: could not record the model of run {}: {error}", turn.id);
        }
    }
    let input = LaunchInput {
        profile: &selected.profile,
        run_id: &turn.id,
        agent_id: &worker.agent_id,
        conversation_id: &turn.conversation_id,
        run_dir: &run_dir,
        role: &worker.role.text,
        prompt,
        tool_url: &worker.tool_url,
        mcp_command: &worker.mcp_command,
        hook_url: &worker.hook_url,
        hook_token: &hook_token,
        hook_command: &worker.hook_command,
        plugin_dir: worker.plugin_dir.as_deref(),
        attachment_dirs,
        engine_model,
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
        .env("EXCUBIA_TOOL_TOKEN", &token)
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

    let exit = async {
        child
            .wait()
            .await
            .map(|status| status.code())
            .map_err(|error| error.to_string())
    };
    let outcome = wait_for_turn(&grant, &worker.chat, exit, worker.timeout).await;
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
        return Ok(Attempt::Done);
    }
    settle(readers).await;
    let secrets = [token.as_str(), hook_token.as_str(), turn.text.as_str()];
    let excerpt = lock(&tail).excerpt(&secrets);
    // A spent free model is not a failed turn: the caller tries the next free model.
    if engine_model.is_some() && is_free_limit(&excerpt) {
        return Ok(Attempt::FreeLimit(excerpt));
    }
    // A turn on the local model or a chosen engine is not the seat's quota: only the default CLI counts.
    let on_default = worker
        .selected
        .as_ref()
        .is_some_and(|default| default.profile.id == selected.profile.id);
    if turn_env == TurnEnv::default() && on_default && looks_like_quota_exhausted(&excerpt) {
        (worker.on_quota_exhausted)();
    }
    // A free model whose CLI ended on its own, without an answer, hands the turn to the next one.
    let ended_alone = engine_model.is_some() && matches!(outcome, Outcome::Exited(_));
    let failure = with_output(outcome.failure_reason(), &excerpt);
    if ended_alone {
        return Ok(Attempt::NoAnswer(failure));
    }
    Err(failure)
}

pub(super) enum Outcome {
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

/// The turn's wait: the answer, a stop, the CLI's exit, or the end of its budget.
/// The budget counts the CLI's active time: while a pending approval of one of the
/// turn's runs waits for the operator, it is paused. The subscription is taken
/// before the first read, so no change is lost between the read and the wait.
pub(super) async fn wait_for_turn(
    grant: &RunGrant,
    chat: &ChatStore,
    exit: impl Future<Output = Result<Option<i32>, String>>,
    timeout: Duration,
) -> Outcome {
    let notified = grant.notify.notified();
    tokio::pin!(notified);
    let stopped = grant.stop.notified();
    tokio::pin!(stopped);
    tokio::pin!(exit);
    let mut frames = chat.subscribe();
    let mut frames_open = true;
    let mut holds = chat.wait_holds_changed();
    let mut budget = ActiveBudget::new(Instant::now(), timeout);
    let (mut waits, mut change) = match read_waits(chat, &grant.run_ids) {
        Ok(read) => read,
        Err(error) => return Outcome::WaitError(error),
    };
    let mut read = false;
    loop {
        if read {
            (waits, change) = match read_waits(chat, &grant.run_ids) {
                Ok(read) => read,
                Err(error) => return Outcome::WaitError(error),
            };
            read = false;
        }
        let now = Instant::now();
        // A card that arrives after the active balance ran out cannot pause it.
        if budget.is_exhausted(now) {
            return Outcome::Timeout;
        }
        budget.set_waiting(now, waits.pending > 0);
        tokio::select! {
            _ = &mut notified => return Outcome::Replied,
            _ = &mut stopped => return Outcome::Stopped,
            status = &mut exit => {
                return match status {
                    Ok(code) => Outcome::Exited(code),
                    Err(error) => Outcome::WaitError(error),
                }
            }
            _ = until(budget.deadline()) => return Outcome::Timeout,
            // A card's level ran out or its TTL passed: read the approvals again.
            _ = until(change) => read = true,
            // A held wait began or ended (the GitHub code): read the waits again.
            _ = holds.changed() => read = true,
            // Only the approval and conversation frames can change a wait; the
            // others, and a lag, are not worth a read (a lag may hide one).
            frame = frames.recv(), if frames_open => match frame {
                Ok(frame) => read = is_wait_frame(&frame.event),
                Err(RecvError::Lagged(_)) => read = true,
                Err(RecvError::Closed) => frames_open = false,
            },
        }
    }
}

/// The turn's waits as the store reads them, and the monotonic instant their next
/// change falls due. The instant is fixed at the read (sampled before it, so a
/// change is never woken late); frames that do not move the wait keep it as it is.
fn read_waits(
    chat: &ChatStore,
    run_ids: &[String],
) -> Result<(ApprovalWaits, Option<Instant>), String> {
    let read_at = Instant::now();
    let waits = chat
        .approval_waits(run_ids)
        .map_err(|error| error.to_string())?;
    let change = waits.next_change.map(|left| read_at + left);
    Ok((waits, change))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::free_models::FREE_MODELS;

    /// Runs `answer_on_engines` with a fake CLI: each model answers with the given outcome.
    async fn answer(
        book: &FreeModels,
        engines: Vec<Option<&'static str>>,
        spent: &[&str],
        tried: &RefCell<Vec<Option<&'static str>>>,
    ) -> Result<(), String> {
        answer_on_engines(book, engines, |engine_model| {
            tried.borrow_mut().push(engine_model);
            let spent_here = engine_model.is_some_and(|model| spent.contains(&model));
            async move {
                Ok(if spent_here {
                    Attempt::FreeLimit("provider.free-limit".to_string())
                } else {
                    Attempt::Done
                })
            }
        })
        .await
    }

    #[tokio::test]
    async fn a_turn_whose_first_free_model_is_spent_completes_on_the_second() {
        let book = FreeModels::default();
        let tried = RefCell::new(Vec::new());
        let engines = vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])];
        let result = answer(&book, engines, &[FREE_MODELS[0]], &tried).await;
        assert_eq!(result, Ok(()));
        assert_eq!(*tried.borrow(), vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])]);
        // The spent model is remembered: the next turn starts on the second one.
        assert_eq!(book.usable(Instant::now()), FREE_MODELS[1..].to_vec());
    }

    #[tokio::test]
    async fn a_turn_with_every_free_model_spent_fails_with_the_exhausted_reason() {
        let book = FreeModels::default();
        let tried = RefCell::new(Vec::new());
        let engines: Vec<Option<&'static str>> = FREE_MODELS.iter().copied().map(Some).collect();
        let result = answer(&book, engines, FREE_MODELS, &tried).await;
        let reason = result.expect_err("every model is spent");
        assert!(reason.starts_with("free_models_exhausted"), "{reason}");
        assert!(reason.contains("provider.free-limit"), "{reason}");
        assert_eq!(tried.borrow().len(), FREE_MODELS.len());
        assert!(book.usable(Instant::now()).is_empty());
    }

    #[tokio::test]
    async fn a_spent_model_is_not_tried_again_inside_its_window() {
        let book = FreeModels::default();
        let now = Instant::now();
        book.mark_exhausted(FREE_MODELS[0], now);
        // The turn starts from the usable list, so the spent model is never an engine of it.
        let engines: Vec<Option<&'static str>> = book.usable(now).into_iter().map(Some).collect();
        let tried = RefCell::new(Vec::new());
        assert_eq!(answer(&book, engines, &[], &tried).await, Ok(()));
        assert_eq!(*tried.borrow(), vec![Some(FREE_MODELS[1])]);
    }

    #[tokio::test]
    async fn a_failure_that_is_not_a_spent_free_tier_is_not_retried() {
        let book = FreeModels::default();
        let calls = RefCell::new(0);
        let result = answer_on_engines(&book, vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])], |_| {
            *calls.borrow_mut() += 1;
            async { Err("process_exit: 1".to_string()) }
        })
        .await;
        assert_eq!(result, Err("process_exit: 1".to_string()));
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(book.usable(Instant::now()), FREE_MODELS.to_vec());
    }

    #[tokio::test]
    async fn a_model_that_ends_without_answering_hands_the_turn_to_the_next_one() {
        let book = FreeModels::default();
        let tried = RefCell::new(Vec::new());
        let engines = vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])];
        let result = answer_on_engines(&book, engines, |engine_model| {
            tried.borrow_mut().push(engine_model);
            async move {
                Ok(if engine_model == Some(FREE_MODELS[0]) {
                    Attempt::NoAnswer("process_exit: 0".to_string())
                } else {
                    Attempt::Done
                })
            }
        })
        .await;
        assert_eq!(result, Ok(()));
        assert_eq!(*tried.borrow(), vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])]);
        // Skipped for this turn only: no model is marked as spent.
        assert_eq!(book.usable(Instant::now()), FREE_MODELS.to_vec());
    }

    #[tokio::test]
    async fn the_last_model_that_ends_without_answering_keeps_its_own_failure() {
        let book = FreeModels::default();
        let tried = RefCell::new(Vec::new());
        let engines = vec![Some(FREE_MODELS[0]), Some(FREE_MODELS[1])];
        let result = answer_on_engines(&book, engines, |engine_model| {
            tried.borrow_mut().push(engine_model);
            async { Ok(Attempt::NoAnswer("process_exit: 0".to_string())) }
        })
        .await;
        assert_eq!(result, Err("process_exit: 0".to_string()));
        assert_eq!(tried.borrow().len(), 2);
        assert_eq!(book.usable(Instant::now()), FREE_MODELS.to_vec());
    }
}
