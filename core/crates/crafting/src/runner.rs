//! What a run is and what comes back. The real runner is in `process.rs`; tests
//! substitute their own.

use crate::{App, CraftError};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Most bytes kept from a CLI's stdout or stderr; the rest is read and dropped.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const DETAIL_CHARS: usize = 200;

/// One CLI call: the program, its arguments as separate values, the folder it
/// runs in and its time limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub app: App,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub timeout: Duration,
}

/// The result of a run that finished in time.
#[derive(Debug, Default, Clone)]
pub struct Captured {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Either stream was longer than `MAX_OUTPUT_BYTES`.
    pub truncated: bool,
}

pub trait Runner {
    /// `Err` only when the CLI could not start or ran out of time; a non-zero exit is a `Captured`.
    fn run(&self, inv: &Invocation) -> Result<Captured, CraftError>;
}

/// The end of a failed run's stderr, cut to 200 characters, with the given
/// folders replaced so the chat never shows a path.
pub(crate) fn failure_detail(stderr: &[u8], scrub: &[&Path]) -> String {
    let mut text = String::from_utf8_lossy(stderr).into_owned();
    for path in scrub {
        let plain = path.display().to_string();
        text = text
            .replace(&plain, "…")
            .replace(&plain.replace('\\', "/"), "…");
    }
    let text = text.trim();
    if text.is_empty() {
        "sin detalle".to_string()
    } else {
        text.chars().take(DETAIL_CHARS).collect()
    }
}
