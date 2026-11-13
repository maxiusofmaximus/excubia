//! The host's audit log: one JSON line per sensitive action, appended to `<data-dir>/audit.jsonl`
//! (`docs/contracts/audit.md`). A full file moves to `audit.1.jsonl`, replacing the previous one, and a
//! new file starts. Writing never fails a request: an error goes to stderr.

use std::fs::{self, OpenOptions};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Local, Utc};
use excubia_api::{AuditEvent, AuditSink};
use serde_json::Value;

pub const AUDIT_FILE: &str = "audit.jsonl";
const ROTATED_FILE: &str = "audit.1.jsonl";
const LIMIT_BYTES: u64 = 5 * 1024 * 1024;
/// The longest value a detail keeps; a longer one is cut.
const DETAIL_CHARS: usize = 200;
/// The device column of `audit tail`: a device id this long or shorter is shown whole.
const DEVICE_COLUMN: usize = 12;

pub struct AuditLog {
    path: PathBuf,
    rotated: PathBuf,
    limit: u64,
    /// Serializes appends and rotations, so two actions never interleave within a line.
    write: Mutex<()>,
}

impl AuditLog {
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

    fn append(&self, event: &AuditEvent) -> std::io::Result<()> {
        let mut event = event.clone();
        for value in event.detail.values_mut() {
            *value = value.chars().take(DETAIL_CHARS).collect();
        }
        let mut line = serde_json::to_vec(&event).map_err(Error::other)?;
        line.push(b'\n');
        let _guard = self.write.lock().unwrap_or_else(PoisonError::into_inner);
        if let Err(error) = self.rotate_for(line.len() as u64) {
            // A log that cannot rotate still takes the line.
            eprintln!("host: could not rotate the audit log: {error}");
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)
    }

    /// Moves the file aside when `incoming` more bytes would pass the limit.
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

impl AuditSink for AuditLog {
    fn record(&self, event: &AuditEvent) {
        // A log that cannot be written must not undo or block the request.
        if let Err(error) = self.append(event) {
            eprintln!("host: could not write the audit log: {error}");
        }
    }
}

/// The last `n` readable lines, oldest first, from the rotated file and then the current one. A line
/// that is not an audit event (a torn write, a hand edit) is skipped.
pub fn tail_lines(data_dir: &Path, n: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for file in [ROTATED_FILE, AUDIT_FILE] {
        let Ok(bytes) = fs::read(data_dir.join(file)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        lines.extend(text.lines().filter_map(readable));
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}

/// `hora local  dispositivo  acción  resultado`, one line per event.
fn readable(line: &str) -> Option<String> {
    let event: Value = serde_json::from_str(line).ok()?;
    let ts = event.get("ts")?.as_i64()?;
    let action = event.get("action")?.as_str()?;
    let outcome = event.get("outcome")?.as_str()?;
    let when = DateTime::<Utc>::from_timestamp(ts, 0)?
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S");
    let device = match event.get("device").and_then(Value::as_str) {
        Some(id) if id.chars().count() > DEVICE_COLUMN => {
            format!(
                "{}…",
                id.chars().take(DEVICE_COLUMN - 1).collect::<String>()
            )
        }
        Some(id) => id.to_string(),
        None => "-".to_string(),
    };
    Some(format!("{when}  {device:<13}{action:<18}{outcome}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_test_dir::TestDir;
    use std::collections::BTreeMap;

    fn event(n: usize) -> AuditEvent {
        AuditEvent {
            ts: 1_700_000_000,
            device: Some("dev-1".to_string()),
            action: "alfred.backend",
            detail: BTreeMap::from([("n", n.to_string())]),
            outcome: "started".to_string(),
        }
    }

    fn lines(path: &Path) -> Vec<Value> {
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
            .collect()
    }

    #[test]
    fn a_line_has_the_contract_fields_in_order_and_ends_with_a_newline() {
        let dir = TestDir::new("audit-line");
        AuditLog::new(&dir).record(&event(1));
        AuditLog::new(&dir).record(&AuditEvent {
            device: None,
            ..event(2)
        });

        let text = fs::read_to_string(dir.join(AUDIT_FILE)).expect("audit file");
        assert_eq!(
            text,
            "{\"ts\":1700000000,\"device\":\"dev-1\",\"action\":\"alfred.backend\",\
             \"detail\":{\"n\":\"1\"},\"outcome\":\"started\"}\n\
             {\"ts\":1700000000,\"device\":null,\"action\":\"alfred.backend\",\
             \"detail\":{\"n\":\"2\"},\"outcome\":\"started\"}\n"
        );
    }

    #[test]
    fn a_detail_value_is_cut_at_200_characters() {
        let dir = TestDir::new("audit-cut");
        let mut long = event(0);
        long.detail.insert("text", "ñ".repeat(250));
        AuditLog::new(&dir).record(&long);

        let line = lines(&dir.join(AUDIT_FILE)).remove(0);
        assert_eq!(
            line["detail"]["text"].as_str().map(|s| s.chars().count()),
            Some(200)
        );
    }

    #[test]
    fn a_full_file_rotates_and_keeps_the_newest_lines_in_order() {
        let dir = TestDir::new("audit-rotate");
        let log = AuditLog::with_limit(&dir, 300);
        for n in 0..10 {
            log.record(&event(n));
        }

        let mut kept = lines(&dir.join(ROTATED_FILE));
        kept.extend(lines(&dir.join(AUDIT_FILE)));
        let numbers: Vec<u32> = kept
            .iter()
            .map(|line| {
                line["detail"]["n"]
                    .as_str()
                    .expect("n")
                    .parse()
                    .expect("number")
            })
            .collect();
        assert!(numbers.len() >= 2, "{numbers:?}");
        assert_eq!(numbers.last(), Some(&9), "{numbers:?}");
        assert!(
            numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "a line went missing: {numbers:?}"
        );
    }

    #[test]
    fn a_write_that_fails_does_not_panic() {
        let dir = TestDir::new("audit-fail");
        // A directory where the log file goes: opening it for append fails.
        fs::create_dir(dir.join(AUDIT_FILE)).expect("blocker");
        AuditLog::new(&dir).record(&event(1));

        assert!(dir.join(AUDIT_FILE).is_dir());
    }

    #[test]
    fn tail_skips_a_corrupt_line_and_reads_the_rotated_file_first() {
        let dir = TestDir::new("audit-tail");
        let good = serde_json::to_string(&event(1)).expect("json");
        fs::write(dir.join(ROTATED_FILE), format!("{good}\n")).expect("rotated");
        fs::write(
            dir.join(AUDIT_FILE),
            format!("{good}\n{{\"ts\":12,\"act\n\n{good}\n"),
        )
        .expect("current");

        let all = tail_lines(&dir, 50);
        assert_eq!(all.len(), 3, "{all:?}");
        assert_eq!(all[0], all[1]);
        // The hour is the machine's local one, so the expected text is built the same way.
        let when = DateTime::<Utc>::from_timestamp(1_700_000_000, 0)
            .expect("time")
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S");
        assert_eq!(
            all[0],
            format!("{when}  {:<13}{:<18}started", "dev-1", "alfred.backend")
        );
        assert_eq!(tail_lines(&dir, 2).len(), 2);
        assert!(tail_lines(&dir.join("missing"), 5).is_empty());
    }
}
