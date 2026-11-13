use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use excubia_chat::{ChatStore, Turn};
use tokio::sync::mpsc;

use crate::profile::{self, SelectedProfile};
use crate::prompt::Role;

use super::seat::TurnEnv;
use super::turn::run_turn;
use super::RunGrant;

pub(super) struct Worker {
    pub(super) chat: Arc<ChatStore>,
    pub(super) receiver: mpsc::Receiver<Turn>,
    pub(super) selected: Option<SelectedProfile>,
    pub(super) engines: Vec<SelectedProfile>,
    pub(super) turn_engine: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub(super) tokens: Arc<Mutex<HashMap<String, Arc<RunGrant>>>>,
    pub(super) mcp_command: PathBuf,
    pub(super) tool_url: String,
    pub(super) hook_url: String,
    pub(super) hook_command: PathBuf,
    pub(super) plugin_dir: Option<PathBuf>,
    pub(super) seat_dir: PathBuf,
    pub(super) role: Role,
    pub(super) timeout: Duration,
    pub(super) agent_id: String,
    pub(super) turn_env: Arc<dyn Fn() -> TurnEnv + Send + Sync>,
    pub(super) on_quota_exhausted: Arc<dyn Fn() + Send + Sync>,
}

pub(super) async fn run_worker(mut worker: Worker) {
    while let Some(turn) = worker.receiver.recv().await {
        process_turn(&worker, turn).await;
    }
}

async fn process_turn(worker: &Worker, turn: Turn) {
    match worker.chat.start_run(&turn.id) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            eprintln!("seat: could not start run {}: {error}", turn.id);
            return;
        }
    }
    let _ = worker.chat.set_seat_status(&worker.agent_id, "working");
    // Fails closed: an engine that is not ready ends the turn; it never falls back to the default.
    let outcome = match turn_profile(worker).await {
        Ok(selected) => run_turn(worker, &selected, &turn).await,
        Err(reason) => Err(reason),
    };
    if let Err(reason) = outcome {
        eprintln!("seat: run {} failed: {reason}", turn.id);
        let _ = worker.chat.fail_run_with_card(&turn.id, &reason);
    }
    let _ = worker.chat.set_seat_status(&worker.agent_id, "idle");
}

/// The CLI that answers this turn: the chosen engine when `alfred-engine` names one, else the
/// seat's default. The engine is probed again here, so a CLI removed or downgraded since the
/// host started fails the turn instead of answering with another provider.
async fn turn_profile(worker: &Worker) -> Result<SelectedProfile, String> {
    let name = (worker.turn_engine)();
    match named_engine(name.as_deref(), &worker.engines)? {
        // `--version` can wait up to 5 s: off the runtime.
        Some(engine) => tokio::task::spawn_blocking(move || probe_engine(engine))
            .await
            .map_err(|error| format!("engine_unavailable: {error}"))?,
        None => worker
            .selected
            .clone()
            .ok_or_else(|| "seat_unavailable".to_string()),
    }
}

/// The listed engine the marker names; `None` when no marker is set. A named engine that is not
/// listed is an error, never a silent default.
fn named_engine(
    name: Option<&str>,
    engines: &[SelectedProfile],
) -> Result<Option<SelectedProfile>, String> {
    let Some(name) = name else {
        return Ok(None);
    };
    engines
        .iter()
        .find(|engine| engine.profile.id.as_str() == name)
        .cloned()
        .map(Some)
        .ok_or_else(|| format!("engine_unavailable: {name} no está listo en este PC"))
}

fn probe_engine(engine: SelectedProfile) -> Result<SelectedProfile, String> {
    let name = engine.profile.id.as_str();
    let Some(version) = profile::probe(&engine.profile) else {
        return Err(format!(
            "engine_unavailable: {name} no responde a --version"
        ));
    };
    if version < engine.profile.min_version {
        return Err(format!(
            "engine_unavailable: {name} {version} es anterior al mínimo {}",
            engine.profile.min_version
        ));
    }
    Ok(SelectedProfile { version, ..engine })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{shipped, CliId, Version};

    fn engine(id: CliId) -> SelectedProfile {
        let profile = shipped()
            .into_iter()
            .find(|profile| profile.id == id)
            .expect("shipped profile");
        SelectedProfile {
            profile,
            version: Version::new(2, 2, 6),
        }
    }

    #[test]
    fn no_marker_keeps_the_seat_default() {
        assert!(named_engine(None, &[engine(CliId::OpenCode)])
            .expect("no marker")
            .is_none());
    }

    #[test]
    fn a_listed_marker_picks_its_engine() {
        let picked = named_engine(Some("opencode"), &[engine(CliId::OpenCode)])
            .expect("listed")
            .expect("an engine");
        assert_eq!(picked.profile.id, CliId::OpenCode);
    }

    #[test]
    fn an_unlisted_marker_fails_closed_and_never_falls_back() {
        // The seat default exists here, but the turn must not use it: the error is the answer.
        let refused = named_engine(Some("opencode"), &[]);
        assert!(refused.is_err_and(|reason| reason.starts_with("engine_unavailable: opencode")));
    }
}
