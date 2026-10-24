//! Test-only seats for the chat flow: a deterministic reply (through the same
//! `complete_run_with_reply` the real MCP tool uses) and a seat that never
//! answers, so a turn can stay queued across a simulated host restart.

use std::sync::Arc;
use std::time::Duration;

use hn_core_chat::{ChatSeat, ChatStore, ToolError, ToolReply, ToolRequest, Turn};

/// Delay before the simulated reply, so the HTTP `202` is observably
/// asynchronous and the turn exists before it is answered.
const REPLY_DELAY: Duration = Duration::from_millis(25);

pub struct FakeSeat {
    chat: Option<Arc<ChatStore>>,
    delay: Duration,
}

impl FakeSeat {
    /// Answer every turn through the same lifecycle method as `chat.reply`.
    pub fn replying(chat: Arc<ChatStore>) -> Self {
        Self {
            chat: Some(chat),
            delay: REPLY_DELAY,
        }
    }

    /// Accept nothing: the run stays queued until a restart marks it
    /// `interrupted`.
    pub fn silent() -> Self {
        Self {
            chat: None,
            delay: Duration::ZERO,
        }
    }
}

#[async_trait::async_trait]
impl ChatSeat for FakeSeat {
    fn try_enqueue(&self, turn: Turn) -> Result<(), hn_core_chat::SeatError> {
        let Some(chat) = self.chat.clone() else {
            return Ok(());
        };
        let delay = self.delay;
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if !chat.start_run(&turn.id).unwrap_or(false) {
                return;
            }
            let text = format!("Respuesta simulada: {}", turn.text);
            if let Err(error) = chat.complete_run_with_reply(&turn.id, &text) {
                eprintln!("fake seat: could not complete {}: {error}", turn.id);
            }
        });
        Ok(())
    }

    async fn tool(&self, _token: &str, _request: ToolRequest) -> Result<ToolReply, ToolError> {
        Err(ToolError::unauthorized("the fake seat has no MCP endpoint"))
    }
}
