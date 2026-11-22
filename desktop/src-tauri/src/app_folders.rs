//! The roaming and local folders Tauri derives from the identifier. The
//! identifier moved from `dev.herdrnotify.desktop` to `dev.excubia.desktop`, so
//! each old folder moves to its new name once, and only when the new one does
//! not exist yet. A failed move keeps the old folder in use: nothing is
//! abandoned or overwritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::{AppHandle, Manager};

const IDENTIFIER: &str = "dev.excubia.desktop";
/// legacy: the identifier before the rename; its folders are only read as the origin of a move.
const LEGACY_IDENTIFIER: &str = "dev.herdrnotify.desktop";
const MOVED_NOTE: &str = "MIGRADO-DESDE.txt";

/// Decided once at startup. Empty when APPDATA or LOCALAPPDATA is missing, and
/// then the Tauri paths are used as they are.
static FOLDERS: OnceLock<Folders> = OnceLock::new();

struct Folders {
    roaming: PathBuf,
    local: PathBuf,
}

/// Decides the folders in use and points WebView2 at its data inside the
/// local one. Call before the Tauri builder; profile mode does not call it.
pub fn settle() {
    let (Some(roaming), Some(local)) = (
        std::env::var_os("APPDATA"),
        std::env::var_os("LOCALAPPDATA"),
    ) else {
        return;
    };
    let folders = Folders {
        roaming: settle_one(Path::new(&roaming), IDENTIFIER, LEGACY_IDENTIFIER),
        local: settle_one(Path::new(&local), IDENTIFIER, LEGACY_IDENTIFIER),
    };
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", folders.local.join("EBWebView"));
    let _ = FOLDERS.set(folders);
}

/// The folder to use under `base`: the new one when it exists; else the old one
/// moved there; else the old one again when the move fails.
fn settle_one(base: &Path, current: &str, legacy: &str) -> PathBuf {
    let (new, old) = (base.join(current), base.join(legacy));
    if new.exists() || !old.exists() {
        return new;
    }
    if fs::rename(&old, &new).is_err() {
        return old;
    }
    let _ = fs::write(
        new.join(MOVED_NOTE),
        format!("Carpeta movida desde {}\n", old.display()),
    );
    new
}

/// The roaming folder, where the config and the local host's data live. On
/// Windows config and data share it.
pub fn roaming_dir(app: &AppHandle) -> Result<PathBuf, String> {
    match FOLDERS.get() {
        Some(folders) => Ok(folders.roaming.clone()),
        None => app.path().app_data_dir().map_err(|error| error.to_string()),
    }
}

/// The local folder, where the logs and WebView2's data live.
pub fn local_dir(app: &AppHandle) -> Result<PathBuf, String> {
    match FOLDERS.get() {
        Some(folders) => Ok(folders.local.clone()),
        None => app
            .path()
            .app_local_data_dir()
            .map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{settle_one, MOVED_NOTE};

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("excubia-folders-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn an_old_folder_is_moved_and_noted() {
        let base = scratch("moves");
        fs::create_dir_all(base.join("old").join("core")).expect("old");
        fs::write(base.join("old").join("config.json"), "{}").expect("write");
        let used = settle_one(&base, "new", "old");
        assert_eq!(used, base.join("new"));
        assert!(used.join("core").is_dir() && used.join("config.json").is_file());
        assert!(!base.join("old").exists());
        assert!(used.join(MOVED_NOTE).is_file());
    }

    #[test]
    fn an_existing_new_folder_is_used_and_the_old_one_is_left_alone() {
        let base = scratch("keeps");
        fs::create_dir_all(base.join("new")).expect("new");
        fs::create_dir_all(base.join("old")).expect("old");
        assert_eq!(settle_one(&base, "new", "old"), base.join("new"));
        assert!(base.join("old").is_dir());
    }

    #[test]
    fn a_fresh_install_uses_the_new_folder_without_creating_it() {
        let base = scratch("fresh");
        assert_eq!(settle_one(&base, "new", "old"), base.join("new"));
        assert!(!base.join("new").exists());
    }
}
