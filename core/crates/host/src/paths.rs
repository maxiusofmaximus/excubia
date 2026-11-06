//! Where the host keeps its data: the trust store and the counters.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use hn_core_persist::migrate::{adopt_dir, chosen_dir};

/// The folder name before the Excubia rename, read until it is moved.
const LEGACY_DIR: &str = "herdr-notify";
const DIR: &str = "excubia";

/// `EXCUBIA_DATA_DIR` overrides the platform default (`HERDR_NOTIFY_DATA_DIR`
/// is read as a fallback); otherwise `%APPDATA%\excubia` on Windows, or
/// `$XDG_CONFIG_HOME/excubia` / `~/.config/excubia` elsewhere.
pub fn default_data_dir() -> Result<PathBuf, String> {
    resolve(|key| std::env::var_os(key))
}

/// Moves the platform data folder from its old name to the new one, once.
/// The host calls it at start-up before reading its arguments, and only when
/// `--data-dir` is not named: an explicit folder is never moved.
pub fn migrate_default_data_dir() {
    migrate(|key| std::env::var_os(key));
}

fn migrate(env: impl Fn(&str) -> Option<OsString>) {
    if explicit_dir(&env).is_none() {
        if let Some((new, old)) = platform_dirs(&env) {
            adopt_dir(&new, &old);
        }
    }
}

fn resolve(env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    if let Some(dir) = explicit_dir(&env) {
        return Ok(dir);
    }
    match platform_dirs(&env) {
        Some((new, old)) => Ok(chosen_dir(&new, &old)),
        None => Err("no data directory: set EXCUBIA_DATA_DIR".to_string()),
    }
}

fn explicit_dir(env: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    env("EXCUBIA_DATA_DIR")
        .or_else(|| env("HERDR_NOTIFY_DATA_DIR"))
        .map(PathBuf::from)
}

/// The new and the old data folder under the platform's config root.
fn platform_dirs(env: &impl Fn(&str) -> Option<OsString>) -> Option<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    if let Some(appdata) = env("APPDATA") {
        return Some(dirs_under(PathBuf::from(appdata)));
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME") {
        return Some(dirs_under(PathBuf::from(xdg)));
    }
    env("HOME").map(|home| dirs_under(PathBuf::from(home).join(".config")))
}

fn dirs_under(root: PathBuf) -> (PathBuf, PathBuf) {
    (root.join(DIR), root.join(LEGACY_DIR))
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

/// The attachment files of the chat, one folder per conversation.
pub fn attachments_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("chat").join("attachments")
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

/// Present while the local model runs: the voice host lets go of its ASR (`local_mode.rs`).
pub fn local_mode_path(data_dir: &Path) -> PathBuf {
    data_dir.join("local-mode")
}

/// Present while Alfred answers with the local model (`alfred_backend.rs`).
pub fn alfred_local_path(data_dir: &Path) -> PathBuf {
    data_dir.join("alfred-local")
}

/// The LAN listener's certificate and key slots (`tls_slots.rs`).
pub fn tls_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tls")
}

/// The restore points of the agents' folders (`restore_points.rs`).
pub fn restore_points_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("restore")
}

pub fn ensure_data_dir(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_persist::migrate::MOVED_NOTE;
    use hn_core_test_dir::TestDir;
    use std::fs;

    #[test]
    fn paths_live_under_the_data_dir() {
        let dir = Path::new("C:/data");
        assert!(devices_path(dir).ends_with("devices.json"));
        assert!(counters_path(dir).ends_with("state.json"));
        assert!(chat_db_path(dir).ends_with("chat.db"));
        assert!(attachments_dir(dir).ends_with(Path::new("chat").join("attachments")));
        assert!(local_mode_path(dir).ends_with("local-mode"));
        assert!(alfred_local_path(dir).ends_with("alfred-local"));
    }

    /// The named variables; every other variable is unset.
    fn env<'a>(vars: &'a [(&'a str, PathBuf)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            vars.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, path)| path.as_os_str().to_owned())
        }
    }

    #[test]
    fn the_explicit_variables_win_and_the_new_one_comes_first() {
        let both = [
            ("HERDR_NOTIFY_DATA_DIR", PathBuf::from("C:/viejo")),
            ("EXCUBIA_DATA_DIR", PathBuf::from("C:/nuevo")),
        ];
        assert_eq!(resolve(env(&both)), Ok(PathBuf::from("C:/nuevo")));
        let old_only = [("HERDR_NOTIFY_DATA_DIR", PathBuf::from("C:/viejo"))];
        assert_eq!(resolve(env(&old_only)), Ok(PathBuf::from("C:/viejo")));
    }

    #[test]
    fn the_platform_folder_is_excubia_and_the_old_one_is_read_until_moved() {
        let root = TestDir::new("paths-read-old");
        let old = root.join(LEGACY_DIR);
        fs::create_dir_all(&old).expect("old folder");
        let vars = [
            ("APPDATA", root.to_path_buf()),
            ("XDG_CONFIG_HOME", root.to_path_buf()),
        ];
        assert_eq!(resolve(env(&vars)), Ok(old));
        let fresh = TestDir::new("paths-fresh");
        let vars = [
            ("APPDATA", fresh.to_path_buf()),
            ("XDG_CONFIG_HOME", fresh.to_path_buf()),
        ];
        assert_eq!(resolve(env(&vars)), Ok(fresh.join(DIR)));
    }

    #[test]
    fn migration_moves_the_old_folder_and_leaves_a_note() {
        let root = TestDir::new("paths-migrate");
        fs::create_dir_all(root.join(LEGACY_DIR)).expect("old folder");
        fs::write(root.join(LEGACY_DIR).join("devices.json"), "[]").expect("file");
        let vars = [
            ("APPDATA", root.to_path_buf()),
            ("XDG_CONFIG_HOME", root.to_path_buf()),
        ];
        migrate(env(&vars));
        assert!(root.join(DIR).join("devices.json").is_file());
        assert!(root.join(DIR).join(MOVED_NOTE).is_file());
        assert!(!root.join(LEGACY_DIR).exists());
        assert_eq!(resolve(env(&vars)), Ok(root.join(DIR)));
    }

    #[test]
    fn an_explicit_folder_is_never_moved() {
        let root = TestDir::new("paths-explicit-no-move");
        fs::create_dir_all(root.join(LEGACY_DIR)).expect("old folder");
        let vars = [
            ("APPDATA", root.to_path_buf()),
            ("EXCUBIA_DATA_DIR", root.join("mine")),
        ];
        migrate(env(&vars));
        assert!(root.join(LEGACY_DIR).is_dir(), "the old folder stays");
        assert!(!root.join(DIR).exists());
        let only_old_var = [
            ("APPDATA", root.to_path_buf()),
            ("HERDR_NOTIFY_DATA_DIR", root.join("mine")),
        ];
        migrate(env(&only_old_var));
        assert!(
            root.join(LEGACY_DIR).is_dir(),
            "the old variable counts as explicit"
        );
    }

    #[test]
    fn when_both_folders_exist_nothing_is_moved_or_mixed() {
        let root = TestDir::new("paths-both");
        fs::create_dir_all(root.join(DIR)).expect("new folder");
        fs::create_dir_all(root.join(LEGACY_DIR)).expect("old folder");
        fs::write(root.join(LEGACY_DIR).join("chat.db"), "old").expect("old file");
        let vars = [
            ("APPDATA", root.to_path_buf()),
            ("XDG_CONFIG_HOME", root.to_path_buf()),
        ];
        migrate(env(&vars));
        assert!(root.join(LEGACY_DIR).join("chat.db").is_file());
        assert!(!root.join(DIR).join("chat.db").exists());
        assert_eq!(resolve(env(&vars)), Ok(root.join(DIR)));
    }
}
