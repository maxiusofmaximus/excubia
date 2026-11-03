//! Alfred's seat: a one-turn FIFO worker, a per-run MCP token registry and the
//! only writer of `queued -> running -> completed|failed|interrupted`.
//!
//! The HTTP route enqueues synchronously so a full queue or a missing CLI is a
//! real `409`, not a late failure. The worker launches one isolated CLI process
//! per turn, passes the prompt on stdin and never reads stdout: the only valid
//! answer arrives through `chat.reply`.

mod grant;
mod output;
mod seat;
mod turn;
mod worker;

#[cfg(test)]
mod tests;

pub(crate) use grant::{Phase, RunGrant};
pub(crate) use seat::{lock, tool_chat_error};
pub use seat::{SeatConfig, SeatService, TurnEnv, CONTEXT_CHARS, CONTEXT_MESSAGES};
