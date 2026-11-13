//! The `local-mode` marker: present while the local model runs (`strata.rs`). The CLI writes
//! and clears it; the daemon watches it and lets go of the voice ASR while it is there.
//! The contract is `docs/contracts/local-backend.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use excubia_api::voice::VoiceService;
use tokio::task::JoinHandle;

use crate::pairing_notice::PairingNotice;
use crate::{paths, strata};

const POLL: Duration = Duration::from_secs(2);
pub const ENTER_NOTICE: &str = "Modo local: voz desactivada mientras corre el modelo local";
pub const EXIT_NOTICE: &str = "Modo local terminado: voz disponible";

pub fn is_on(data_dir: &Path) -> bool {
    paths::local_mode_path(data_dir).exists()
}

pub fn set(data_dir: &Path) -> io::Result<()> {
    paths::ensure_data_dir(data_dir)?;
    excubia_persist::write_atomic(&paths::local_mode_path(data_dir), b"")
}

pub fn clear(data_dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(paths::local_mode_path(data_dir)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Startup, before the watch: a marker with no model running is left by a run that died, so it
/// goes, and the notice says voice is back. A marker with a model running stays.
pub fn recover(data_dir: &Path, notices: &PairingNotice) -> io::Result<()> {
    if is_on(data_dir) && !strata::is_running() {
        clear(data_dir)?;
        announce(notices, EXIT_NOTICE);
    }
    Ok(())
}

/// Checks the marker every 2 s and moves the voice service with it. One notice per change.
pub fn watch(
    voice: Arc<VoiceService>,
    notices: Arc<PairingNotice>,
    data_dir: PathBuf,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut on = false;
        let mut ticks = tokio::time::interval(POLL);
        loop {
            ticks.tick().await;
            let want = is_on(&data_dir);
            if want == on {
                continue;
            }
            on = want;
            let (voice, notices) = (Arc::clone(&voice), Arc::clone(&notices));
            // Suspend and resume wait for a note in flight, so they stay off the runtime.
            let _ = tokio::task::spawn_blocking(move || {
                if want {
                    voice.suspend();
                    announce(&notices, ENTER_NOTICE);
                } else {
                    voice.resume();
                    announce(&notices, EXIT_NOTICE);
                }
            })
            .await;
        }
    })
}

/// The notice is a courtesy: its failure is reported and never stops the switch.
fn announce(notices: &PairingNotice, text: &str) {
    if let Err(error) = notices.notify(text) {
        eprintln!("host: could not write the local-mode notice: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_test_dir::TestDir;

    #[test]
    fn the_marker_is_set_seen_and_cleared_and_clearing_twice_is_fine() {
        let dir = TestDir::new("local-mode");
        assert!(!is_on(&dir));
        set(&dir).expect("set");
        assert!(is_on(&dir));
        clear(&dir).expect("clear");
        assert!(!is_on(&dir));
        clear(&dir).expect("clearing an absent marker is not an error");
    }
}
