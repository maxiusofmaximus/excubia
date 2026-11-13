//! Moving a data folder to its new name, once, without losing or mixing data.
//!
//! The new folder wins whenever it exists, and nothing is ever merged or
//! deleted here. The old folder is moved only when the new one is missing; if
//! that move fails, the old folder stays in use.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The note left in a folder that was moved from its old name.
pub const MOVED_NOTE: &str = "MIGRADO-DESDE.txt";

/// The folder to use without moving anything: `new` unless only `old` exists.
pub fn chosen_dir(new: &Path, old: &Path) -> PathBuf {
    if new.exists() || !old.exists() {
        new.to_path_buf()
    } else {
        old.to_path_buf()
    }
}

/// Moves `old` to `new` when only `old` exists, and returns the folder to use.
/// Every notice goes to stderr.
pub fn adopt_dir(new: &Path, old: &Path) -> PathBuf {
    if new.exists() {
        if old.exists() {
            eprintln!(
                "aviso: existen {} y {}; se usa la primera y la otra queda sin tocar",
                new.display(),
                old.display()
            );
        }
        return new.to_path_buf();
    }
    if !old.exists() {
        return new.to_path_buf();
    }
    match move_dir(old, new) {
        Ok(()) => new.to_path_buf(),
        Err(error) => {
            eprintln!(
                "aviso: no se pudo mover {} a {} ({error}); se sigue usando la carpeta vieja",
                old.display(),
                new.display()
            );
            old.to_path_buf()
        }
    }
}

fn move_dir(old: &Path, new: &Path) -> std::io::Result<()> {
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(old, new)?;
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let note = format!(
        "Carpeta movida desde {}\nfecha (segundos unix): {unix}\n",
        old.display()
    );
    if let Err(error) = fs::write(new.join(MOVED_NOTE), note) {
        eprintln!("aviso: la carpeta se movió, pero no se pudo escribir {MOVED_NOTE}: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_test_dir::TestDir;

    fn folder_with(dir: &Path, file: &str) {
        fs::create_dir_all(dir).expect("folder");
        fs::write(dir.join(file), "data").expect("file");
    }

    #[test]
    fn a_missing_new_folder_takes_the_old_one_over() {
        let root = TestDir::new("migrate-rename");
        let (new, old) = (root.join("excubia"), root.join("herdr-notify"));
        folder_with(&old, "devices.json");
        assert_eq!(chosen_dir(&new, &old), old, "read the old one until moved");
        assert_eq!(adopt_dir(&new, &old), new);
        assert!(new.join("devices.json").is_file());
        assert!(!old.exists(), "the old folder is moved, not copied");
        let note = fs::read_to_string(new.join(MOVED_NOTE)).expect("note");
        assert!(note.contains(&old.display().to_string()), "{note}");
        assert!(note.contains("fecha"), "{note}");
    }

    #[test]
    fn when_both_exist_the_new_one_wins_and_nothing_is_mixed() {
        let root = TestDir::new("migrate-both");
        let (new, old) = (root.join("excubia"), root.join("herdr-notify"));
        folder_with(&new, "new.json");
        folder_with(&old, "old.json");
        assert_eq!(chosen_dir(&new, &old), new);
        assert_eq!(adopt_dir(&new, &old), new);
        assert!(
            old.join("old.json").is_file(),
            "the old folder is untouched"
        );
        assert!(!new.join("old.json").exists(), "nothing is merged");
        assert!(!new.join(MOVED_NOTE).exists());
    }

    #[test]
    fn with_nothing_on_disk_the_new_path_is_returned_and_not_created() {
        let root = TestDir::new("migrate-none");
        let (new, old) = (root.join("excubia"), root.join("herdr-notify"));
        assert_eq!(adopt_dir(&new, &old), new);
        assert!(!new.exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_blocked_old_folder_stays_in_use_and_nothing_is_lost() {
        let root = TestDir::new("migrate-blocked");
        let (new, old) = (root.join("excubia"), root.join("herdr-notify"));
        folder_with(&old, "chat.db");
        // An open file without delete sharing blocks the rename on Windows.
        let open = fs::File::open(old.join("chat.db")).expect("open");
        assert_eq!(adopt_dir(&new, &old), old);
        drop(open);
        assert!(old.join("chat.db").is_file());
        assert!(!new.exists(), "a failed move leaves no half copy");
    }
}
