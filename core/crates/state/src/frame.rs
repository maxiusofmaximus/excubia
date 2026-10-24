//! SSE change frames and the event names on the wire, kept from herdr's
//! vocabulary so the existing gateway-facing clients recognise them.

use std::sync::Arc;

pub const EVENT_STATUS: &str = "pane.agent_status_changed";
pub const EVENT_CREATED: &str = "pane.created";
pub const EVENT_CLOSED: &str = "pane.closed";
pub const EVENT_UPDATED: &str = "pane.updated";

/// A change frame handed to SSE subscribers.
#[derive(Clone, Debug)]
pub struct Frame {
    pub event: String,
    pub body: serde_json::Value,
}

pub type SharedFrame = Arc<Frame>;
