//! Where the host keeps its data: the trust store and the counters.

use std::path::{Path, PathBuf};

/// `HERDR_NOTIFY_DATA_DIR` overrides the platform default (used by tests and
/// the demo scripts); otherwise `%APPDATA%\herdr-notify` on Windows, or
/// `$XDG_CONFIG_HOME/herdr-notify` / `~/.config/herdr-notify` elsewhere.
pub fn default_data_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("HERDR_NOTIFY_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return Ok(PathBuf::from(appdata).join("herdr-notify"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("herdr-notify"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".config").join("herdr-notify"));
    }
    Err("no data directory: set HERDR_NOTIFY_DATA_DIR".to_string())
}

pub fn devices_path(data_dir: &Path) -> PathBuf {
    data_dir.join("devices.json")
}

pub fn counters_path(data_dir: &Path) -> PathBuf {
    data_dir.join("state.json")
}

/// The persistent chat database (`orchestrator-chat.md` §1).
pub fn chat_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("chat.db")
}

/// The closed list of agents `POST /v1/workspaces` may start.
pub fn agents_path(data_dir: &Path) -> PathBuf {
    data_dir.join("agents.toml")
}

/// What to restore after a restart.
pub fn snapshot_path(data_dir: &Path) -> PathBuf {
    data_dir.join("session.json")
}

/// The token agent hooks authenticate with.
pub fn hook_token_path(data_dir: &Path) -> PathBuf {
    data_dir.join("hook-token")
}

/// The LAN listener's certificate and key slots (`tls_slots.rs`).
pub fn tls_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tls")
}

pub fn ensure_data_dir(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_live_under_the_data_dir() {
        let dir = Path::new("C:/data");
        assert!(devices_path(dir).ends_with("devices.json"));
        assert!(counters_path(dir).ends_with("state.json"));
        assert!(chat_db_path(dir).ends_with("chat.db"));
    }
}
