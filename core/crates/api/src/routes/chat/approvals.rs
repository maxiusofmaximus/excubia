//! The approval routes (`orchestrator-chat.md` §3 and §5).
//!
//! Two surfaces share one domain and never mix their credentials: the hook
//! creates, polls and consumes an approval with the host token (loopback
//! only), and only a signed device with the `APPROVE` grant decides it. The
//! plugin polls the state; it never sees the payload and it never decides.

mod access;
mod bodies;
mod bounds;
mod create;
mod device;
mod poll;

pub use create::create;
pub use device::{decision, list};
pub use poll::{consume, state};
