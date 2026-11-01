use std::sync::Mutex;

use hn_core_chat::{ChatStore, SeatActor, Turn};
use tokio::sync::Notify;

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
    /// Woken by a user stop: the CLI is killed and the turn closes without a card.
    pub(crate) stop: Notify,
}

impl RunGrant {
    pub(super) fn new(turn: &Turn) -> Self {
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
            stop: Notify::new(),
        }
    }
}

/// The run's hook token lives exactly as long as the turn: every exit path of
/// `run_turn` revokes it, so a stale hook can never answer for a closed run.
pub(super) struct HookTokenGuard<'a> {
    pub(super) chat: &'a ChatStore,
    pub(super) run_id: &'a str,
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
