//! Atomic file persistence for the herdr-notify core.
//!
//! Every file this core writes (the trust store, the state counters, the
//! session snapshot, the hook token) goes through the same rule: write a
//! sibling `.tmp`, flush it to disk, then rename over the target. A crash
//! mid-write must never leave a half-written file that later reads as a
//! smaller revision, an emptier device list, or a shorter snapshot.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Write `bytes` to `path` atomically.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_via_tmp(path, bytes, fs::OpenOptions::new())
}

/// Like `write_atomic`, for a private key: owner-only on Unix. Windows keeps
/// the ACL of the profile directory the file lands in.
pub fn write_secret_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    owner_only(&mut options);
    write_via_tmp(path, bytes, options)
}

#[cfg(unix)]
fn owner_only(options: &mut fs::OpenOptions) {
    std::os::unix::fs::OpenOptionsExt::mode(options, 0o600);
}

#[cfg(not(unix))]
fn owner_only(_options: &mut fs::OpenOptions) {}

fn write_via_tmp(path: &Path, bytes: &[u8], mut options: fs::OpenOptions) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut handle = options.write(true).create(true).truncate(true).open(&tmp)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Pretty JSON plus a trailing newline, written atomically.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    write_atomic(path, json.as_bytes())
}

/// Read and parse a JSON file. `Ok(None)` means the file does not exist;
/// every other failure is the caller's to turn into its own fallback.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> std::io::Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let value = serde_json::from_str(&raw)?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The sibling path a write lands on before the rename.
pub fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        revision: u64,
    }

    /// The scratch directory, which the caller keeps alive, and the file in it.
    fn temp_path(name: &str) -> (TestDir, PathBuf) {
        let dir = TestDir::new(&format!("persist-tests-{name}"));
        let path = dir.join("sample.json");
        (dir, path)
    }

    #[test]
    fn a_round_trip_leaves_no_tmp_behind() {
        let (_dir, path) = temp_path("round-trip");
        write_json_atomic(&path, &Sample { revision: 7 }).expect("write");
        let loaded: Sample = read_json(&path).expect("read").expect("some");
        assert_eq!(loaded, Sample { revision: 7 });
        assert!(!tmp_path(&path).exists(), "the tmp file is renamed away");
    }

    #[test]
    fn a_missing_file_is_none_and_corruption_is_an_error() {
        let (_dir, path) = temp_path("missing");
        let _ = fs::remove_file(&path);
        assert!(read_json::<Sample>(&path).expect("read").is_none());
        fs::write(&path, "not json").expect("write");
        assert!(read_json::<Sample>(&path).is_err());
    }
}
