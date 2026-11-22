//! One supervised agent: its pty, its emulated screen, its detection loop.
//!
//! The loop is event-driven: it reacts to pty output, to the child's exit and
//! to hook reports, evaluates the screen at most once per interval while
//! output is flowing, and always evaluates the trailing edge, so a dialog that
//! appears in the last burst (and then the agent waits) is still seen.
//!
//! A pane can also be `Restored`: a record of a previous host run, with no
//! process behind it. Its last screen is served by `read`, and anything that
//! would drive the agent is refused.

mod detect;
mod handle;
mod monitor;

#[cfg(test)]
mod tests;

use std::time::Duration;

/// How often the screen is re-evaluated while output keeps arriving.
const DETECT_INTERVAL: Duration = Duration::from_millis(150);
/// How many screen lines the detector sees.
const DETECTION_LINES: usize = 60;
/// Scrollback kept by the terminal emulator.
pub(crate) const SCROLLBACK_LINES: usize = 2000;
/// How long to keep reading after the child exits (ConPTY can hold the
/// output pipe open a little longer).
const EXIT_DRAIN_GRACE: Duration = Duration::from_millis(500);
/// How long an orderly stop waits before forcing the child.
const STOP_GRACE: Duration = Duration::from_secs(2);

pub(crate) use detect::status_of;
pub(crate) use handle::Session;
pub(crate) use monitor::{monitor_agent, Monitor};
