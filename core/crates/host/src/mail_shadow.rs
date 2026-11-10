//! Modo sombra del triaje (F1): tras cada `mail.triage` el host pregunta a Laya por
//! cada correo leído y anota su respuesta en `mail-shadow.jsonl`. Nada de esto llega a
//! Alfred ni toca el buzón, y la línea nunca lleva el contenido del correo.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use excubia_core_mail::inbox::ReceivedMessage;
use excubia_core_mail::received::Checks;
use excubia_core_mail::triage::{verdict, Level, Signal, Verdict};
use serde::Serialize;

use crate::mail_sidecar::{Sidecar, SidecarAnswer, SidecarEndpoint, SidecarError};

pub const SHADOW_FILE: &str = "mail-shadow.jsonl";
const SHADOW_LIMIT_BYTES: u64 = 5 * 1024 * 1024;
const SIDECAR_DEADLINE: Duration = Duration::from_secs(5);
const PREVIEW_CHARS: usize = 500;

/// The messages one triage read, in the order Alfred received them.
pub struct Batch {
    pub account: String,
    pub folder: &'static str,
    pub messages: Vec<ReceivedMessage>,
}

pub struct Shadow {
    sidecar: Sidecar,
    log: ShadowLog,
    busy: AtomicBool,
    skipped: AtomicU64,
}

impl Shadow {
    /// The shadow of a host: the sidecar at `endpoint`, its log in `data_dir`.
    pub fn for_host(endpoint: SidecarEndpoint, data_dir: &Path) -> Arc<Self> {
        let sidecar = Sidecar::new(endpoint, SIDECAR_DEADLINE);
        let log = ShadowLog::new(data_dir.join(SHADOW_FILE), SHADOW_LIMIT_BYTES);
        Self::new(sidecar, log)
    }

    pub fn new(sidecar: Sidecar, log: ShadowLog) -> Arc<Self> {
        Arc::new(Self {
            sidecar,
            log,
            busy: AtomicBool::new(false),
            skipped: AtomicU64::new(0),
        })
    }

    /// Batches dropped because another pass was still running.
    pub fn skipped(&self) -> u64 {
        self.skipped.load(Ordering::Relaxed)
    }

    /// Starts the pass in the background and returns at once. Only one pass runs:
    /// a batch that finds one running is dropped and counted, never queued.
    pub fn start(self: &Arc<Self>, batch: Batch) {
        let Some(claim) = self.claim() else {
            self.skipped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        tokio::spawn(async move {
            claim.0.run(&batch).await;
        });
    }

    fn claim(self: &Arc<Self>) -> Option<Claim> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Claim(Arc::clone(self)))
    }

    /// One sidecar call per message, in order, each with its own line.
    async fn run(&self, batch: &Batch) {
        for message in &batch.messages {
            let found = verdict(&message.checks);
            let (status, sidecar) =
                outcome(self.sidecar.triage(&Ficha::new(message, &found)).await);
            let line = ShadowLine {
                ts: now_rfc3339(),
                account: &batch.account,
                folder: batch.folder,
                message_id: &message.id,
                level: found.level,
                signals: &found.signals,
                status,
                sidecar,
            };
            if let Err(error) = self.log.append(&line) {
                eprintln!("host: mail shadow log not written: {error}");
            }
        }
    }
}

/// Frees the one pass slot when the pass ends, even if it panics.
struct Claim(Arc<Shadow>);

impl Drop for Claim {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

/// The JSON-lines log, renamed to `<name>.1` (replacing the previous one) once it
/// reaches `limit` bytes.
pub struct ShadowLog {
    path: PathBuf,
    rotated: PathBuf,
    limit: u64,
}

impl ShadowLog {
    pub fn new(path: PathBuf, limit: u64) -> Self {
        let rotated = path.with_extension("jsonl.1");
        Self {
            path,
            rotated,
            limit,
        }
    }

    pub fn append<T: Serialize>(&self, line: &T) -> io::Result<()> {
        let mut text = serde_json::to_string(line).map_err(io::Error::other)?;
        text.push('\n');
        if fs::metadata(&self.path).is_ok_and(|meta| meta.len() >= self.limit) {
            fs::rename(&self.path, &self.rotated)?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(text.as_bytes())
    }
}

/// What the sidecar is told about one message: the fields of `sidecar/mail-triage/ficha.py`.
/// `contact_known` and `preferences` are null until the contacts and preferences exist.
#[derive(Serialize)]
struct Ficha<'a> {
    from_name: &'a str,
    from_address: &'a str,
    subject: &'a str,
    preview: String,
    checks: &'a Checks,
    deterministic: &'a Verdict,
    link_count: usize,
    contact_known: Option<bool>,
    preferences: Option<String>,
}

impl<'a> Ficha<'a> {
    fn new(message: &'a ReceivedMessage, found: &'a Verdict) -> Self {
        Self {
            from_name: &message.from_name,
            from_address: &message.from_address,
            subject: &message.subject,
            preview: message.body_text.chars().take(PREVIEW_CHARS).collect(),
            checks: &message.checks,
            deterministic: found,
            link_count: message.links.len(),
            contact_known: None,
            preferences: None,
        }
    }
}

#[derive(Serialize)]
struct ShadowLine<'a> {
    ts: String,
    account: &'a str,
    folder: &'a str,
    message_id: &'a str,
    level: Level,
    signals: &'a [Signal],
    status: Status,
    sidecar: Option<SidecarAnswer>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Ok,
    Timeout,
    Unavailable,
    BadResponse,
}

fn outcome(result: Result<SidecarAnswer, SidecarError>) -> (Status, Option<SidecarAnswer>) {
    match result {
        Ok(answer) => (Status::Ok, Some(answer)),
        Err(SidecarError::Timeout) => (Status::Timeout, None),
        Err(SidecarError::Unavailable) => (Status::Unavailable, None),
        Err(SidecarError::BadResponse) => (Status::BadResponse, None),
    }
}

fn now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    rfc3339(seconds)
}

/// `YYYY-MM-DDThh:mm:ssZ` for Unix seconds, from the civil-calendar algorithm of
/// Howard Hinnant (the same days-to-date rule the standard library lacks).
pub(crate) fn rfc3339(unix_seconds: i64) -> String {
    let days = unix_seconds.div_euclid(86_400) + 719_468;
    let seconds_of_day = unix_seconds.rem_euclid(86_400);
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3600,
        seconds_of_day % 3600 / 60,
        seconds_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    #[test]
    fn unix_seconds_become_utc_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_000_000_000), "2001-09-09T01:46:40Z");
    }
}
