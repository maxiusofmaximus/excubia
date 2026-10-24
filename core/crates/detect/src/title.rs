//! OSC title tracking.
//!
//! herdr reads the window title through its ghostty terminal (`osc_title`
//! region). The prototype's terminal (`vt100`) does not expose it, so we scan
//! the raw pty stream for `OSC 0;` / `OSC 1;` / `OSC 2;` and keep the last
//! title. Terminated by BEL or ST (`ESC \`), and state survives chunk
//! boundaries.

const MAX_TITLE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Osc,
    OscEsc,
}

pub struct TitleTracker {
    state: State,
    title: String,
    pending: Vec<u8>,
}

impl Default for TitleTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleTracker {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            title: String::new(),
            pending: Vec::new(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Feed raw pty bytes. Titles are decoded leniently (invalid UTF-8 is
    /// replaced, not dropped).
    pub fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match self.state {
                State::Ground => {
                    if byte == 0x1b {
                        self.state = State::Esc;
                    }
                }
                State::Esc => {
                    self.state = if byte == b']' {
                        self.pending.clear();
                        State::Osc
                    } else if byte == 0x1b {
                        State::Esc
                    } else {
                        State::Ground
                    };
                }
                State::Osc => match byte {
                    0x07 => self.finish(),
                    0x1b => self.state = State::OscEsc,
                    _ => {
                        if self.pending.len() < MAX_TITLE_BYTES {
                            self.pending.push(byte);
                        }
                    }
                },
                State::OscEsc => {
                    if byte == b'\\' {
                        self.finish();
                    } else {
                        // Not a string terminator: keep collecting, keeping
                        // the ESC in case it starts one after all.
                        if self.pending.len() < MAX_TITLE_BYTES {
                            self.pending.push(0x1b);
                            self.pending.push(byte);
                        }
                        self.state = State::Osc;
                    }
                }
            }
        }
    }

    fn finish(&mut self) {
        self.state = State::Ground;
        let body = std::mem::take(&mut self.pending);
        let text = String::from_utf8_lossy(&body);
        // `0;` and `2;` set the window title; `1;` sets the icon name. A
        // `3;`-style payload (or anything unrecognised) is not a title.
        let Some(rest) = text.strip_prefix("0;").or_else(|| text.strip_prefix("2;")) else {
            return;
        };
        if !rest.is_empty() {
            self.title = rest.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_osc_titles_across_chunks() {
        let mut tracker = TitleTracker::new();
        tracker.push(b"\x1b]0;build");
        tracker.push(b"ing...\x07");
        assert_eq!(tracker.title(), "building...");

        // ST-terminated, OSC 2.
        tracker.push(b"\x1b]2;done\x1b\\");
        assert_eq!(tracker.title(), "done");

        // Icon name (OSC 1) is ignored.
        tracker.push(b"\x1b]1;icon\x07");
        assert_eq!(tracker.title(), "done");

        // Other escape sequences do not corrupt the state.
        tracker.push(b"\x1b[31mred\x1b[0m");
        assert_eq!(tracker.title(), "done");
    }
}
