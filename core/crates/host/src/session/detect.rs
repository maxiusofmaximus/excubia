//! Screen and hook state helpers: the store's status for a detected state, the
//! terminal query responder and the last-lines text helpers.

use excubia_core_detect::DetectedState;
use excubia_core_state::Status;

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
pub(super) struct QueryResponder {
    tail: Vec<u8>,
}

impl QueryResponder {
    pub(super) fn scan(&mut self, chunk: &[u8]) -> bool {
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
pub(super) fn recent_text(screen: &vt100::Screen, lines: usize) -> String {
    let contents = screen.contents();
    last_lines(&contents, lines)
}

/// Last `lines` lines of a block of text.
pub(super) fn last_lines(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}
