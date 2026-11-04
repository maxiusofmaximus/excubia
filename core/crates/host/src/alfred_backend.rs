//! Alfred's model: Claude (the default) or the local Strata model, chosen by hand with
//! `host alfred use`. The marker `<data-dir>/alfred-local` means "local". Each turn of Alfred
//! is a new CLI process, so the seat reads the marker when a turn starts (`turn_env`). The
//! contract is `docs/contracts/local-backend.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use hn_core_seat::TurnEnv;
use tokio::task::JoinHandle;

use crate::cli::AlfredCommand;
use crate::pairing_notice::PairingNotice;
use crate::paths;
use crate::strata::{self, State};

const POLL: Duration = Duration::from_secs(2);
/// Removed from a turn's CLI while the local model answers: Claude would send it to the cloud.
const API_KEY: &str = "ANTHROPIC_API_KEY";
pub const ENTER_NOTICE: &str = "Alfred usa ahora el modelo local";
pub const EXIT_NOTICE: &str = "Volvió a Claude";
/// At most one quota notice in this span, so a run of failed turns posts one message.
const QUOTA_NOTICE_EVERY: Duration = Duration::from_secs(30 * 60);
pub const QUOTA_NOTICE: &str = "Claude alcanzó su límite de uso. Para seguir con el modelo local, en el PC: host alfred local (tarda unos 4 min, apaga la voz y necesita ~34 GB de RAM libre)";

pub fn is_on(data_dir: &Path) -> bool {
    paths::alfred_local_path(data_dir).exists()
}

pub fn set(data_dir: &Path) -> io::Result<()> {
    paths::ensure_data_dir(data_dir)?;
    hn_core_persist::write_atomic(&paths::alfred_local_path(data_dir), b"")
}

pub fn clear(data_dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(paths::alfred_local_path(data_dir)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The environment that points Claude Code at the local server. The `claude-local` entry and
/// Alfred's turns both take it from here.
pub fn local_env() -> Vec<(&'static str, String)> {
    vec![
        ("ANTHROPIC_BASE_URL", strata::endpoint()),
        ("ANTHROPIC_MODEL", strata::MODEL.to_string()),
        // The placeholder token is the one value the catalogue allows under a credential
        // name (see `catalog.rs`); the local server checks no key.
        ("ANTHROPIC_AUTH_TOKEN", "strata-local".to_string()),
        ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", "65536".to_string()),
        ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "4096".to_string()),
    ]
}

/// What a turn of Alfred adds to its CLI: nothing without the marker, the local model's
/// environment and no API key with it.
pub fn turn_env(data_dir: &Path) -> TurnEnv {
    if !is_on(data_dir) {
        return TurnEnv::default();
    }
    TurnEnv {
        set: local_env()
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
        remove: vec![API_KEY.to_string()],
    }
}

/// Startup, before the watch: a marker with no model running is left by a run that died, so it
/// goes, and the notice says Alfred is back on Claude.
pub fn recover(data_dir: &Path, notices: &PairingNotice) -> io::Result<()> {
    if is_on(data_dir) && !strata::is_running() {
        clear(data_dir)?;
        announce(notices, EXIT_NOTICE);
    }
    Ok(())
}

/// Checks the marker every 2 s and posts one notice per change. Starts from the marker as it is,
/// so a marker that was already there at startup does not announce itself.
pub fn watch(notices: Arc<PairingNotice>, data_dir: PathBuf) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut on = is_on(&data_dir);
        let mut ticks = tokio::time::interval(POLL);
        loop {
            ticks.tick().await;
            let want = is_on(&data_dir);
            if want == on {
                continue;
            }
            on = want;
            let notices = Arc::clone(&notices);
            let text = if want { ENTER_NOTICE } else { EXIT_NOTICE };
            // The notice writes to the chat store, which blocks: off the runtime.
            let _ = tokio::task::spawn_blocking(move || announce(&notices, text)).await;
        }
    })
}

/// The notice is a courtesy: a failure is reported and never stops the switch.
fn announce(notices: &PairingNotice, text: &str) {
    if let Err(error) = notices.notify(text) {
        eprintln!("host: could not write the alfred-local notice: {error}");
    }
}

/// When the last quota notice went out. In memory: a restart may post one more.
#[derive(Default)]
pub struct QuotaNotices {
    last: Mutex<Option<Instant>>,
}

impl QuotaNotices {
    /// True, and recorded, when no notice went out less than `every` before `now`.
    fn allow(&self, now: Instant, every: Duration) -> bool {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(at) = *last {
            if now.saturating_duration_since(at) < every {
                return false;
            }
        }
        *last = Some(now);
        true
    }
}

/// The seat's quota hook: posts `QUOTA_NOTICE` at most once per `QUOTA_NOTICE_EVERY`, and
/// never while the local model is on (a quota failure then is not Claude's).
pub fn quota_hook(notices: Arc<PairingNotice>, data_dir: PathBuf) -> Arc<dyn Fn() + Send + Sync> {
    let limiter = QuotaNotices::default();
    Arc::new(move || {
        if is_on(&data_dir) || !limiter.allow(Instant::now(), QUOTA_NOTICE_EVERY) {
            return;
        }
        let notices = Arc::clone(&notices);
        // The notice writes to the chat store, which blocks: off the runtime.
        tokio::task::spawn_blocking(move || announce(&notices, QUOTA_NOTICE));
    })
}

/// Run one `host alfred` subcommand.
pub fn run(command: AlfredCommand, data_dir: &Path) -> Result<()> {
    match command {
        AlfredCommand::UseLocal => {
            if !matches!(strata::state(&strata::binary()?), Ok(State::Ready)) {
                bail!("Strata no está listo; arranca Strata antes: host strata start");
            }
            set(data_dir).context("write the alfred-local marker")?;
            println!("Alfred: modelo local");
        }
        AlfredCommand::Local => {
            // Starts the model (the voice yields) and marks Alfred only once it answers.
            let bin = strata::binary()?;
            strata::start(&bin, data_dir).context("start the local model")?;
            if !matches!(strata::state(&bin), Ok(State::Ready)) {
                bail!("Strata no quedó listo; mira `host strata status`");
            }
            set(data_dir).context("write the alfred-local marker")?;
            println!("Alfred: modelo local");
        }
        AlfredCommand::UseClaude => {
            clear(data_dir).context("remove the alfred-local marker")?;
            println!("Alfred: Claude");
        }
        AlfredCommand::Status => {
            let model = if is_on(data_dir) {
                "modelo local"
            } else {
                "Claude"
            };
            println!("Alfred: {model}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    #[test]
    fn the_marker_is_set_and_cleared_and_clearing_twice_is_fine() {
        let dir = TestDir::new("alfred-local");
        assert!(!is_on(&dir));
        set(&dir).expect("set");
        assert!(is_on(&dir));
        clear(&dir).expect("clear");
        clear(&dir).expect("clearing an absent marker is not an error");
        assert!(!is_on(&dir));
    }

    #[test]
    fn the_turn_env_is_empty_without_the_marker_and_local_with_it() {
        let dir = TestDir::new("alfred-local-env");
        assert_eq!(turn_env(&dir), TurnEnv::default());
        set(&dir).expect("set");
        let env = turn_env(&dir);
        assert_eq!(env.remove, vec![API_KEY.to_string()]);
        assert!(env
            .set
            .iter()
            .any(|(key, value)| key == "ANTHROPIC_BASE_URL" && value == &strata::endpoint()));
        assert!(!env.set.iter().any(|(key, _)| key == API_KEY));
    }

    #[test]
    fn quota_notices_come_at_most_once_per_span() {
        let notices = QuotaNotices::default();
        let span = Duration::from_secs(30 * 60);
        let start = Instant::now();
        assert!(notices.allow(start, span));
        assert!(!notices.allow(start + Duration::from_secs(60), span));
        assert!(!notices.allow(start + span - Duration::from_secs(1), span));
        assert!(notices.allow(start + span, span));
        assert!(!notices.allow(start + span + Duration::from_secs(1), span));
    }
}
