//! Authoritative state store for the herdr-notify core.
//!
//! Implements the read model of `docs/contracts/state.md` for the agents this
//! host supervises: monotonic `revision`, per-status `counts`, the triage
//! ranking, and a global `state_change_seq` (herdr's counter, bumped on every
//! real status transition). `done` is derived, exactly as herdr derives it:
//! `idle` + the operator has not seen it yet.
//!
//! `revision` and `state_change_seq` are persisted (when a path is given), so
//! a client that reconnects after a host restart never sees them go backwards.

mod frame;
mod payload;
mod persist;
mod record;
mod status;
mod store;

pub use frame::{Frame, SharedFrame, EVENT_CLOSED, EVENT_CREATED, EVENT_STATUS, EVENT_UPDATED};
pub use payload::{StatePayload, TriageRow};
pub use record::AgentRecord;
pub use status::Status;
pub use store::{NewAgent, StateStore};
