//! Documents the chat downloads to open them. The bytes are staged in a folder
//! of the app's cache and handed to the system's own program; the webview names
//! only the attachment, never a path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// How long a staged copy stays on disk; the next start removes older ones.
pub const KEEP_FOR: Duration = Duration::from_secs(24 * 60 * 60);
const FOLDER: &str = "documentos";
const MAX_NAME_CHARS: usize = 120;
const FALLBACK_NAME: &str = "documento";

pub fn folder(cache: &Path) -> PathBuf {
    cache.join(FOLDER)
}

/// `<attachment id>-<name>`. The name keeps no separators, no `..`, no reserved
/// device name and at most 120 characters, so it cannot leave the folder.
pub fn staged_name(attachment_id: &str, name: Option<&str>) -> String {
    format!("{attachment_id}-{}", clean_name(name.unwrap_or_default()))
}

fn clean_name(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|character| {
            if character.is_control() || "/\\:*?\"<>|".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = mapped.trim().trim_end_matches(['.', ' ']);
    let limited: String = trimmed
        .replace("..", "_")
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let name = limited.trim_end_matches(['.', ' ']);
    match name {
        "" => FALLBACK_NAME.to_string(),
        name if is_reserved(name) => format!("_{name}"),
        name => name.to_string(),
    }
}

/// Windows refuses these device names as file names, with or without an extension.
fn is_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let bytes = stem.as_bytes();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (bytes.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && (b'1'..=b'9').contains(&bytes[3]))
}

/// Writes the bytes into the folder and returns their path.
pub fn stage(
    cache: &Path,
    attachment_id: &str,
    name: Option<&str>,
    bytes: &[u8],
) -> io::Result<PathBuf> {
    let dir = folder(cache);
    fs::create_dir_all(&dir)?;
    let path = dir.join(staged_name(attachment_id, name));
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Removes the files in `dir` last written more than `KEEP_FOR` before `now`;
/// returns how many went. A folder that does not exist has nothing to remove.
pub fn sweep(dir: &Path, now: SystemTime) -> io::Result<usize> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry?;
        let modified = entry.metadata()?.modified()?;
        let stale = now.duration_since(modified).is_ok_and(|age| age > KEEP_FOR);
        if stale && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Hands the file to the system's own program. Only a failure to start the
/// opener counts: its exit code is not a reliable signal on every system.
pub fn open_with_system(path: &Path) -> io::Result<()> {
    let mut child = opener(path).spawn()?;
    // Reaped in the background: some openers stay alive until the app they start closes.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(windows)]
fn opener(path: &Path) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg(path);
    command
}

#[cfg(target_os = "macos")]
fn opener(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn opener(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::time::{Duration, SystemTime};

    use excubia_desktop_core::chat::validate_prefixed_id;

    use super::{is_reserved, staged_name, sweep, KEEP_FOR};

    fn scratch(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|age| age.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("excubia-{label}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn hostile_names_stay_inside_the_folder() {
        for raw in [
            "../../etc/passwd",
            "a/b\\c.xlsx",
            "C:\\Windows\\x.pdf",
            "..",
            "x\u{0}y.pdf",
            "   ",
        ] {
            let name = staged_name("att_1", Some(raw));
            assert!(
                !name.contains(['/', '\\', ':', '\0']),
                "{raw:?} -> {name:?}"
            );
            assert!(!name.contains(".."), "{raw:?} -> {name:?}");
        }
    }

    #[test]
    fn windows_device_names_get_an_underscore() {
        assert_eq!(staged_name("att_1", Some("CON")), "att_1-_CON");
        assert_eq!(staged_name("att_1", Some("con.txt")), "att_1-_con.txt");
        assert_eq!(staged_name("att_1", Some("LPT9.md")), "att_1-_LPT9.md");
        assert!(!is_reserved("COM0"), "COM0 is not a device");
        assert!(!is_reserved("CONSOLA.txt"));
    }

    #[test]
    fn names_are_capped_and_never_end_in_a_dot() {
        let long = staged_name("att_1", Some(&"a".repeat(200)));
        assert_eq!(long.len(), "att_1-".len() + 120);
        assert_eq!(staged_name("att_1", Some("informe...")), "att_1-informe");
        assert_eq!(staged_name("att_1", None), "att_1-documento");
        assert_eq!(
            staged_name("att_1", Some("Informe Q1.xlsx")),
            "att_1-Informe Q1.xlsx"
        );
    }

    #[test]
    fn an_id_that_carries_a_path_is_refused_before_any_file_is_named() {
        assert!(validate_prefixed_id("att_../../x", "att_", "adjunto").is_err());
        assert!(validate_prefixed_id("att_1/x", "att_", "adjunto").is_err());
        assert!(validate_prefixed_id("att_1", "att_", "adjunto").is_ok());
    }

    #[test]
    fn files_older_than_a_day_go_and_recent_ones_stay() {
        let dir = scratch("sweep");
        fs::create_dir_all(&dir).expect("folder");
        let old = dir.join("att_old-informe.xlsx");
        let recent = dir.join("att_new-informe.xlsx");
        fs::write(&old, b"x").expect("old");
        fs::write(&recent, b"y").expect("recent");
        let now = SystemTime::now();
        File::options()
            .write(true)
            .open(&old)
            .and_then(|file| file.set_modified(now - KEEP_FOR - Duration::from_secs(3600)))
            .expect("age the old file");

        assert_eq!(sweep(&dir, now).expect("sweep"), 1);
        assert!(!old.exists());
        assert!(recent.exists());
        fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_missing_folder_has_nothing_to_sweep() {
        let dir = scratch("missing");
        assert_eq!(sweep(&dir, SystemTime::now()).expect("sweep"), 0);
    }
}
