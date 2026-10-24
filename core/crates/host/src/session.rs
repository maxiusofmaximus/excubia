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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hn_core_detect::{DetectedState, Detector, TitleTracker};
use hn_core_pty::{ExitInfo, PtySession, WriteHandle};
use hn_core_state::{StateStore, Status};
use tokio::sync::{mpsc, Notify};

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
    /// Signalled when a client asked this pane to stop.
    stop: Arc<Notify>,
}

impl Session {
    pub fn live(pty: PtySession, parser: Arc<Mutex<vt100::Parser>>) -> Self {
        Self {
            io: SessionIo::Live(LiveIo { pty, parser }),
            hook: Mutex::new(None),
            stop: Arc::new(Notify::new()),
        }
    }

    pub fn restored(text: String) -> Self {
        Self {
            io: SessionIo::Restored(RestoredIo {
                text: Mutex::new(text),
            }),
            hook: Mutex::new(None),
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

/// Everything the monitor needs; one struct so the signature stays readable.
pub(crate) struct Monitor {
    pub store: Arc<StateStore>,
    pub sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
    pub pane_id: String,
    pub output: mpsc::UnboundedReceiver<Vec<u8>>,
    pub exit: mpsc::UnboundedReceiver<ExitInfo>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub title: Arc<Mutex<TitleTracker>>,
    pub detector: Option<Arc<Detector>>,
    pub writer: WriteHandle,
    pub hook_ttl: Duration,
    /// Signalled when a hook report arrives, so the monitor re-evaluates at
    /// once (and arms the expiry check).
    pub hook_reports: Arc<Notify>,
    /// Signalled when the client asks this pane to stop.
    pub stop: Arc<Notify>,
    pub dirty: Arc<Notify>,
    pub shutting_down: Arc<AtomicBool>,
}

impl Monitor {
    fn session(&self) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .get(&self.pane_id)
            .cloned()
    }
}

/// Follow one agent until its process is gone.
///
/// On exit the pane is removed from the state list and from the sessions map:
/// a pane that no longer exists must not appear, and actions against it answer
/// the contract's typed error. During a shutdown the pane is kept, because the
/// snapshot it will be restored from is being written.
pub(crate) async fn monitor_agent(mut monitor: Monitor) {
    let mut next_detect: Option<tokio::time::Instant> = None;
    let mut next_hook_check: Option<tokio::time::Instant> = None;
    let mut exit_seen = false;
    let mut drain_deadline: Option<tokio::time::Instant> = None;
    let mut stop_deadline: Option<tokio::time::Instant> = None;
    let mut queries = QueryResponder::default();

    loop {
        let trailing = async {
            match next_detect {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        let hook_check = async {
            match next_hook_check {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        let stop_escalation = async {
            match stop_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        let drain = async {
            match drain_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            biased;
            info = monitor.exit.recv(), if !exit_seen => {
                exit_seen = true;
                let _ = info;
                drain_deadline = Some(tokio::time::Instant::now() + EXIT_DRAIN_GRACE);
            }
            _ = drain, if drain_deadline.is_some() => break,
            _ = monitor.stop.notified() => {
                // Orderly close first; the exit event ends this normally.
                if let Some(session) = monitor.session() {
                    session.kill();
                }
                stop_deadline = Some(tokio::time::Instant::now() + STOP_GRACE);
            }
            _ = stop_escalation, if stop_deadline.is_some() => {
                // The child ignored the close: force it and stop waiting for
                // an exit event that may never come.
                if let Some(session) = monitor.session() {
                    session.force_kill();
                }
                stop_deadline = None;
                drain_deadline = Some(tokio::time::Instant::now() + EXIT_DRAIN_GRACE);
            }
            _ = monitor.hook_reports.notified() => {
                next_hook_check = detect_once(&monitor).await;
                next_detect = None;
            }
            _ = hook_check, if next_hook_check.is_some() => {
                // A hook report has expired: the screen speaks again.
                next_hook_check = detect_once(&monitor).await;
            }
            _ = trailing, if next_detect.is_some() => {
                next_hook_check = detect_once(&monitor).await;
                next_detect = None;
            }
            chunk = monitor.output.recv() => match chunk {
                Some(bytes) => {
                    if queries.scan(&bytes) {
                        // ConPTY (inherit-cursor) asks the terminal for the
                        // cursor position and waits for the reply before it
                        // streams output; herdr answers this from its
                        // emulator the same way.
                        let (row, col) = {
                            let parser = monitor.parser.lock().expect("parser poisoned");
                            parser.screen().cursor_position()
                        };
                        let reply = format!("\x1b[{};{}R", row + 1, col + 1);
                        let _ = monitor.writer.write(reply.as_bytes());
                    }
                    {
                        let mut parser = monitor.parser.lock().expect("parser poisoned");
                        parser.process(&bytes);
                    }
                    let current_title = {
                        let mut tracker = monitor.title.lock().expect("title poisoned");
                        tracker.push(&bytes);
                        tracker.title().to_string()
                    };
                    if !current_title.is_empty() {
                        monitor.store.set_terminal_title(&monitor.pane_id, &current_title).await;
                    }
                    monitor.dirty.notify_one();
                    let now = tokio::time::Instant::now();
                    match next_detect {
                        Some(deadline) if now < deadline => {}
                        _ => {
                            next_hook_check = detect_once(&monitor).await;
                            next_detect = Some(now + DETECT_INTERVAL);
                        }
                    }
                }
                None => break,
            },
        }
    }

    if !monitor.shutting_down.load(Ordering::SeqCst) {
        // The process is gone: the pane goes with it.
        monitor
            .sessions
            .lock()
            .expect("sessions poisoned")
            .remove(&monitor.pane_id);
        monitor.store.remove_agent(&monitor.pane_id).await;
        monitor.dirty.notify_one();
    }
}

/// Re-evaluate the pane's state, hook first. Returns when the next hook
/// check is due, if a hook report is currently fresh.
async fn detect_once(monitor: &Monitor) -> Option<tokio::time::Instant> {
    let next_hook_check = monitor
        .sessions
        .lock()
        .expect("sessions poisoned")
        .get(&monitor.pane_id)
        .cloned()
        .and_then(|session| {
            session
                .fresh_hook_remaining(monitor.hook_ttl, Instant::now())
                .map(|remaining| tokio::time::Instant::now() + remaining)
        });
    let Some(detector) = monitor.detector.as_deref() else {
        return next_hook_check;
    };
    // A fresh hook report wins over the screen: the agent knows its own state
    // better than a pattern does.
    if let Some(hook_state) = fresh_hook(monitor) {
        monitor
            .store
            .set_status(&monitor.pane_id, status_of(hook_state))
            .await;
        return next_hook_check;
    }
    let screen_tail = {
        let parser = monitor.parser.lock().expect("parser poisoned");
        recent_text(parser.screen(), DETECTION_LINES)
    };
    let osc_title = {
        let tracker = monitor.title.lock().expect("title poisoned");
        tracker.title().to_string()
    };
    let Some(detection) = detector.detect(&screen_tail, &osc_title) else {
        // No rule matched: keep the previous state (herdr behaves the same;
        // hooks cover the agents whose screens carry no idle signal).
        return next_hook_check;
    };
    monitor
        .store
        .set_status(&monitor.pane_id, status_of(detection.state))
        .await;
    next_hook_check
}

/// The hook's state for a pane, while it is fresh.
pub(crate) fn fresh_hook(monitor: &Monitor) -> Option<DetectedState> {
    monitor
        .session()?
        .fresh_hook(monitor.hook_ttl, Instant::now())
}

/// The store's status for a detected screen state.
pub(crate) fn status_of(state: DetectedState) -> Status {
    match state {
        DetectedState::Idle => Status::Idle,
        DetectedState::Working => Status::Working,
        DetectedState::Blocked => Status::Blocked,
        DetectedState::Unknown => Status::Unknown,
    }
}

/// Detects terminal queries (`ESC[6n`) in the raw output stream, keeping a
/// small tail so a sequence split across chunks is still seen.
#[derive(Default)]
struct QueryResponder {
    tail: Vec<u8>,
}

impl QueryResponder {
    fn scan(&mut self, chunk: &[u8]) -> bool {
        let mut joined = Vec::with_capacity(self.tail.len() + chunk.len());
        joined.extend_from_slice(&self.tail);
        joined.extend_from_slice(chunk);
        let found = joined.windows(4).any(|window| window == b"\x1b[6n");
        let keep = joined.len().min(3);
        self.tail = joined[joined.len() - keep..].to_vec();
        found
    }
}

/// Last `lines` lines of the emulated screen (scrollback included).
pub(crate) fn recent_text(screen: &vt100::Screen, lines: usize) -> String {
    let contents = screen.contents();
    last_lines(&contents, lines)
}

/// Last `lines` lines of a block of text.
pub(crate) fn last_lines(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_responder_sees_split_sequences() {
        let mut responder = QueryResponder::default();
        assert!(!responder.scan(b"\x1b["));
        assert!(responder.scan(b"6n"));
        assert!(!responder.scan(b"hola"));
    }

    #[test]
    fn a_restored_session_serves_its_text_and_refuses_writes() {
        let session = Session::restored("uno\ndos\ntres".to_string());
        assert_eq!(session.read(2), "dos\ntres");
        assert!(session.write(b"y").is_err());
        assert!(!session.is_live());
    }

    #[test]
    fn a_hook_report_is_fresh_until_its_ttl_passes() {
        let session = Session::restored(String::new());
        let now = Instant::now();
        session.set_hook(DetectedState::Blocked, now);
        assert_eq!(
            session.fresh_hook(Duration::from_secs(5), now),
            Some(DetectedState::Blocked)
        );
        assert_eq!(
            session.fresh_hook(Duration::from_secs(5), now + Duration::from_secs(6)),
            None
        );
        assert!(session
            .fresh_hook_remaining(Duration::from_secs(5), now + Duration::from_secs(6))
            .is_none());
    }
}
