//! The one installation path: lock, download, digest, extract, probe, swap.
//!
//! Every step either completes or leaves the tool folder as it was. The marker is
//! written last, inside the staged copy, so a folder that exists with the marker
//! has passed every check. A failed step names itself through [`InstallError::code`].

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::archive;
use crate::fetch::Fetch;
use crate::layout::{join_relative, Layout, MARKER};
use crate::manifest::Distribution;

const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Runs an executable's `--version`. A trait so tests need no real CLI for
/// unit cases; production uses [`ExecProbe`].
pub trait Probe: Send + Sync {
    fn version_output(&self, exe: &Path) -> Result<String, String>;
}

/// Runs `exe --version` with no inherited environment beyond the Windows basics.
pub struct ExecProbe;

impl Probe for ExecProbe {
    fn version_output(&self, exe: &Path) -> Result<String, String> {
        let mut command = Command::new(exe);
        command
            .arg("--version")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in ["SYSTEMROOT", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut child = command.spawn().map_err(|error| format!("start: {error}"))?;
        let started = Instant::now();
        loop {
            match child.try_wait().map_err(|error| format!("wait: {error}"))? {
                Some(status) if status.success() => break,
                Some(status) => return Err(format!("exit {status}")),
                None if started.elapsed() >= PROBE_TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timed out".into());
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        }
        let mut text = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout
                .read_to_string(&mut text)
                .map_err(|error| format!("read: {error}"))?;
        }
        Ok(text)
    }
}

/// What an install call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Installed { version: String },
    AlreadyInstalled { version: String },
}

/// Why an install stopped. Each variant names the step that failed.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("another installation of this profile is running")]
    Busy,
    #[error("the download failed: {0}")]
    Download(String),
    #[error("the download has {actual} bytes; the release has {expected}")]
    Size { expected: u64, actual: u64 },
    #[error("the SHA256 does not match the pinned release")]
    Digest { expected: String, actual: String },
    #[error("the archive cannot be extracted: {0}")]
    Extract(String),
    #[error("the archive does not contain {0}")]
    Layout(String),
    #[error("the executable does not report {expected}: {detail}")]
    Version { expected: String, detail: String },
    #[error("the files could not be put in place: {0}")]
    Swap(#[source] io::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

impl InstallError {
    /// A stable code for logs and cards.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Download(_) => "download_failed",
            Self::Size { .. } => "size_mismatch",
            Self::Digest { .. } => "digest_mismatch",
            Self::Extract(_) => "extract_failed",
            Self::Layout(_) => "layout_missing",
            Self::Version { .. } => "version_mismatch",
            Self::Swap(_) => "swap_failed",
            Self::Io(_) => "io_failed",
        }
    }
}

pub struct Installer<'a> {
    pub layout: &'a Layout,
    pub fetch: &'a dyn Fetch,
    pub probe: &'a dyn Probe,
}

impl Installer<'_> {
    /// Installs `dist` unless the verified copy is already there.
    pub fn install(&self, dist: &Distribution) -> Result<Outcome, InstallError> {
        fs::create_dir_all(self.layout.root())?;
        let _lock = Lock::take(self.layout.root())?;
        if let crate::layout::Status::Ready { version } = self.layout.status(dist) {
            return Ok(Outcome::AlreadyInstalled { version });
        }
        clear_leftovers(self.layout.root());

        let stamp = nanos();
        let staging = self
            .layout
            .root()
            .join(".staging")
            .join(format!("{}-{stamp}", dist.dir));
        let _staged = Removed(staging.clone());
        fs::create_dir_all(&staging)?;

        let archive_path = staging.join("download.zip");
        self.fetch
            .fetch(&dist.url, dist.size, &archive_path)
            .map_err(InstallError::Download)?;
        let actual_size = fs::metadata(&archive_path)?.len();
        if actual_size != dist.size {
            return Err(InstallError::Size {
                expected: dist.size,
                actual: actual_size,
            });
        }
        let actual = sha256_hex(&archive_path)?;
        if actual != dist.sha256 {
            return Err(InstallError::Digest {
                expected: dist.sha256.clone(),
                actual,
            });
        }

        let tree = staging.join("tree");
        archive::extract(&archive_path, &tree).map_err(InstallError::Extract)?;
        let exe = join_relative(&tree, &dist.exe);
        if !exe.is_file() {
            return Err(InstallError::Layout(dist.exe.clone()));
        }
        let output = self
            .probe
            .version_output(&exe)
            .map_err(|detail| InstallError::Version {
                expected: dist.probe_marker.clone(),
                detail,
            })?;
        if !output.contains(&dist.probe_marker) {
            return Err(InstallError::Version {
                expected: dist.probe_marker.clone(),
                detail: first_line(&output),
            });
        }

        let marker = serde_json::json!({ "version": dist.version, "sha256": dist.sha256 });
        excubia_persist::write_atomic(&tree.join(MARKER), marker.to_string().as_bytes())?;
        swap_in(self.layout, dist, &tree, stamp)?;
        Ok(Outcome::Installed {
            version: dist.version.clone(),
        })
    }
}

/// Puts the staged tree where the tool lives. The previous copy is kept until the new
/// one is in place, and put back if that fails.
fn swap_in(layout: &Layout, dist: &Distribution, tree: &Path, stamp: u128) -> Result<(), InstallError> {
    let target = layout.tool_dir(dist);
    let previous = layout
        .root()
        .join(format!(".old-{}-{stamp}", dist.dir));
    let had_previous = target.exists();
    if had_previous {
        fs::rename(&target, &previous).map_err(InstallError::Swap)?;
    }
    if let Err(error) = fs::rename(tree, &target) {
        if had_previous {
            let _ = fs::rename(&previous, &target);
        }
        return Err(InstallError::Swap(error));
    }
    if had_previous {
        let _ = fs::remove_dir_all(&previous);
    }
    Ok(())
}

/// Removes what an earlier run left behind. Best effort: nothing here is the verified copy.
fn clear_leftovers(root: &Path) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(".old-") {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    let _ = fs::remove_dir_all(root.join(".staging"));
}

/// The SHA256 of a file, as lowercase hex.
pub fn sha256_hex(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().chars().take(120).collect()
}

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Removes a folder when dropped, whatever the outcome.
struct Removed(PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One install at a time per profile: an OS lock on a file that stays in place. The handle is
/// held until the install ends (a slow download never looks dead), and a process that dies
/// releases it by itself, so the file needs no age check and is never removed between owners.
struct Lock(#[allow(dead_code)] fs::File); // held for its OS lock until drop

impl Lock {
    fn take(root: &Path) -> Result<Self, InstallError> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".install.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(fs::TryLockError::WouldBlock) => Err(InstallError::Busy),
            Err(fs::TryLockError::Error(error)) => Err(InstallError::Io(error)),
        }
    }
}
