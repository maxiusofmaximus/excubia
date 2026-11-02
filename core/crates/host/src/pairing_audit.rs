//! The pairing audit on disk: one JSON line per `POST /v1/pair` decision, appended to
//! `<data-dir>/pairing-audit.jsonl`. The API hands over records; this file is the sink.
//! A full file moves to `pairing-audit.jsonl.1` (replacing the previous one) and a new
//! file starts, so the log stays near the limit and no line is lost while rotating.

use std::fs::{self, OpenOptions};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use hn_core_api::{PairingAttempt, PairingAudit};

pub const AUDIT_FILE: &str = "pairing-audit.jsonl";
const ROTATED_FILE: &str = "pairing-audit.jsonl.1";
const LIMIT_BYTES: u64 = 1024 * 1024;

pub struct JsonlPairingAudit {
    path: PathBuf,
    rotated: PathBuf,
    limit: u64,
    /// Serializes appends and rotations, so two decisions never interleave within a line.
    write: Mutex<()>,
}

impl JsonlPairingAudit {
    pub fn new(data_dir: &Path) -> Self {
        Self::with_limit(data_dir, LIMIT_BYTES)
    }

    /// `limit` is the size an append may not pass; tests use a small one.
    pub fn with_limit(data_dir: &Path, limit: u64) -> Self {
        Self {
            path: data_dir.join(AUDIT_FILE),
            rotated: data_dir.join(ROTATED_FILE),
            limit,
            write: Mutex::new(()),
        }
    }

    fn append(&self, attempt: &PairingAttempt) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(attempt).map_err(Error::other)?;
        line.push(b'\n');
        let _guard = self.write.lock().unwrap_or_else(PoisonError::into_inner);
        if let Err(error) = self.rotate_for(line.len() as u64) {
            // A log that cannot rotate still takes the line.
            eprintln!("host: could not rotate the pairing audit: {error}");
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)
    }

    /// Moves the file aside when `incoming` more bytes would pass the limit. Runs under the
    /// write lock, so no append can land between the rename and the new file.
    fn rotate_for(&self, incoming: u64) -> std::io::Result<()> {
        let size = match fs::metadata(&self.path) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if size > 0 && size + incoming > self.limit {
            fs::rename(&self.path, &self.rotated)?;
        }
        Ok(())
    }
}

impl PairingAudit for JsonlPairingAudit {
    fn record(&self, attempt: &PairingAttempt) {
        // A log that cannot be written must not undo or block the pairing.
        if let Err(error) = self.append(attempt) {
            eprintln!("host: could not write the pairing audit: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_api::{PairOutcome, PairVia};
    use hn_core_test_dir::TestDir;

    fn attempt(label: &str) -> PairingAttempt {
        PairingAttempt {
            ts: 0.0,
            outcome: PairOutcome::Rejected,
            device_id: None,
            label: Some(label.to_owned()),
            via: PairVia::Code,
            source_ip: "127.0.0.1".to_owned(),
        }
    }

    fn labels(path: &Path) -> Vec<String> {
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).expect("one JSON line");
                value["label"].as_str().expect("label").to_owned()
            })
            .collect()
    }

    #[test]
    fn a_full_file_rotates_and_keeps_the_newest_lines_in_order() {
        let dir = TestDir::new("audit-rotate");
        let sink = JsonlPairingAudit::with_limit(&dir, 300);
        for n in 0..10 {
            sink.record(&attempt(&format!("d{n}")));
        }

        let mut kept = labels(&dir.join(ROTATED_FILE));
        kept.extend(labels(&dir.join(AUDIT_FILE)));
        let numbers: Vec<u32> = kept
            .iter()
            .map(|label| label[1..].parse().expect("number"))
            .collect();
        assert!(numbers.len() >= 2, "{kept:?}");
        assert_eq!(numbers.last(), Some(&9), "{kept:?}");
        assert!(
            numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "a line went missing: {kept:?}"
        );
    }

    #[test]
    fn a_rotation_that_fails_still_writes_the_line() {
        let dir = TestDir::new("audit-rotate-fail");
        // A directory where the rotated file goes makes the rename fail.
        fs::create_dir(dir.join(ROTATED_FILE)).expect("blocker");
        let sink = JsonlPairingAudit::with_limit(&dir, 1);
        sink.record(&attempt("a"));
        sink.record(&attempt("b"));

        assert_eq!(labels(&dir.join(AUDIT_FILE)), ["a", "b"]);
    }
}
