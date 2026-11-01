//! The pane's monitor: follows one agent's output, exit and hook reports, and
//! keeps its state in the store up to date.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hn_core_detect::{DetectedState, Detector, TitleTracker};
use hn_core_pty::{ExitInfo, WriteHandle};
use hn_core_state::StateStore;
use tokio::sync::{mpsc, Notify};

use super::detect::{recent_text, status_of, QueryResponder};
use super::handle::Session;
use super::{DETECTION_LINES, DETECT_INTERVAL, EXIT_DRAIN_GRACE, STOP_GRACE};

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
fn fresh_hook(monitor: &Monitor) -> Option<DetectedState> {
    monitor
        .session()?
        .fresh_hook(monitor.hook_ttl, Instant::now())
}
