//! The running side of a pane: the live pty with its emulator, or the text of
//! a pane restored after a restart, and the last hook report.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use excubia_detect::DetectedState;
use excubia_pty::PtySession;
use tokio::sync::Notify;

use super::detect::{last_lines, recent_text};

/// The running side of a pane.
pub(crate) struct LiveIo {
    pub pty: PtySession,
    pub parser: Arc<Mutex<vt100::Parser>>,
}

/// The remembered side of a pane restored after a restart.
pub(crate) struct RestoredIo {
    pub text: Mutex<String>,
}

pub(crate) enum SessionIo {
    Live(LiveIo),
    Restored(RestoredIo),
}

/// The last hook report for a pane, with when it arrived.
pub(crate) struct HookReport {
    pub state: DetectedState,
    pub reported_at: Instant,
}

pub(crate) struct Session {
    pub io: SessionIo,
    pub hook: Mutex<Option<HookReport>>,
    /// The folder the agent runs in: what its restore points save. `None` for a
    /// restored pane, which has no process to write to.
    pub workdir: Option<PathBuf>,
    /// Signalled when a client asked this pane to stop.
    stop: Arc<Notify>,
}

impl Session {
    pub fn live(pty: PtySession, parser: Arc<Mutex<vt100::Parser>>, workdir: PathBuf) -> Self {
        Self {
            io: SessionIo::Live(LiveIo { pty, parser }),
            hook: Mutex::new(None),
            workdir: Some(workdir),
            stop: Arc::new(Notify::new()),
        }
    }

    pub fn restored(text: String) -> Self {
        Self {
            io: SessionIo::Restored(RestoredIo {
                text: Mutex::new(text),
            }),
            hook: Mutex::new(None),
            workdir: None,
            stop: Arc::new(Notify::new()),
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self.io, SessionIo::Live(_))
    }

    /// The child's process id, for diagnostics and the stop check.
    pub fn pid(&self) -> Option<u32> {
        match &self.io {
            SessionIo::Live(io) => io.pty.pid(),
            SessionIo::Restored(_) => None,
        }
    }

    /// Ask the pane's monitor to close the child.
    pub fn request_stop(&self) {
        self.stop.notify_one();
    }

    /// The signal the monitor waits on.
    pub fn stop_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.stop)
    }

    /// Terminate a live agent. A restored pane has nothing to kill.
    pub fn kill(&self) {
        if let SessionIo::Live(io) = &self.io {
            let _ = io.pty.kill();
        }
    }

    /// Force the child when the orderly close did not take.
    pub fn force_kill(&self) {
        if let SessionIo::Live(io) = &self.io {
            let _ = io.pty.kill_force();
        }
    }

    /// Write to the agent, as if typed. A restored pane has no agent to drive.
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        match &self.io {
            SessionIo::Live(io) => io
                .pty
                .write(bytes)
                .map_err(|error| format!("pty write failed: {error}")),
            SessionIo::Restored(_) => Err("the pane has no running agent".to_string()),
        }
    }

    /// The last `lines` lines of what this pane last showed.
    pub fn read(&self, lines: usize) -> String {
        match &self.io {
            SessionIo::Live(io) => {
                let parser = io.parser.lock().expect("parser poisoned");
                recent_text(parser.screen(), lines)
            }
            SessionIo::Restored(io) => {
                let text = io.text.lock().expect("text poisoned");
                last_lines(&text, lines)
            }
        }
    }

    /// What a snapshot should keep for this pane.
    pub fn snapshot_text(&self, lines: usize) -> String {
        self.read(lines)
    }

    /// The hook's state while the report is fresh, if any.
    pub fn fresh_hook(&self, ttl: Duration, now: Instant) -> Option<DetectedState> {
        let hook = self.hook.lock().expect("hook poisoned");
        hook.as_ref()
            .filter(|report| now.duration_since(report.reported_at) < ttl)
            .map(|report| report.state)
    }

    /// How long the current hook report stays fresh, if it is fresh now.
    pub fn fresh_hook_remaining(&self, ttl: Duration, now: Instant) -> Option<Duration> {
        let hook = self.hook.lock().expect("hook poisoned");
        hook.as_ref().and_then(|report| {
            let age = now.duration_since(report.reported_at);
            (age < ttl).then(|| ttl - age)
        })
    }

    pub fn set_hook(&self, state: DetectedState, now: Instant) {
        let mut hook = self.hook.lock().expect("hook poisoned");
        *hook = Some(HookReport {
            state,
            reported_at: now,
        });
    }
}
