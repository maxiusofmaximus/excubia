use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hn_core_chat::{ChatStore, Turn};
use tokio::sync::mpsc;

use crate::profile::SelectedProfile;
use crate::prompt::Role;

use super::seat::TurnEnv;
use super::turn::run_turn;
use super::RunGrant;

pub(super) struct Worker {
    pub(super) chat: Arc<ChatStore>,
    pub(super) receiver: mpsc::Receiver<Turn>,
    pub(super) selected: Option<SelectedProfile>,
    pub(super) tokens: Arc<Mutex<HashMap<String, Arc<RunGrant>>>>,
    pub(super) mcp_command: PathBuf,
    pub(super) tool_url: String,
    pub(super) hook_url: String,
    pub(super) hook_command: PathBuf,
    pub(super) seat_dir: PathBuf,
    pub(super) role: Role,
    pub(super) timeout: Duration,
    pub(super) agent_id: String,
    pub(super) turn_env: Arc<dyn Fn() -> TurnEnv + Send + Sync>,
    pub(super) on_quota_exhausted: Arc<dyn Fn() + Send + Sync>,
}

pub(super) async fn run_worker(mut worker: Worker) {
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
        eprintln!("seat: run {} failed: {reason}", turn.id);
        let _ = worker.chat.fail_run_with_card(&turn.id, &reason);
    }
    let _ = worker.chat.set_seat_status(&worker.agent_id, "idle");
}
