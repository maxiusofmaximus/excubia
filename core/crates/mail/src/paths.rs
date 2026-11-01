//! Which local files may be attached, decided before a byte leaves the machine.
//!
//! The requested path is canonicalized first, so `..` and links are resolved
//! before any rule reads it. A file under the workspaces root is allowed. A
//! file anywhere else needs approval, and an approval is bound to the path
//! *and* the content: its digest changes if a single byte does. Devices, UNC
//! paths and links that leave the root are refused outright, as are
//! directories, empty files and files over the limit.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::config::HARD_MAX_ATTACHMENT_BYTES;
use crate::error::MailError;
use crate::redact::Redacted;

const ACTION: &str = "mail.attach_path";
const HASH_BUFFER_BYTES: usize = 64 * 1024;

/// A regular file that passed every rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedFile {
    pub path: PathBuf,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

impl CheckedFile {
    /// The name Graph shows for the attachment.
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_name()?.to_str()
    }
}

/// A file outside the workspace, waiting for the user's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathApproval {
    pub digest: String,
    pub file: CheckedFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDecision {
    Allowed(CheckedFile),
    NeedsApproval(PathApproval),
}

impl PathDecision {
    pub fn file(&self) -> &CheckedFile {
        match self {
            Self::Allowed(file) => file,
            Self::NeedsApproval(pending) => &pending.file,
        }
    }
}

/// The attachment rules. Approvals given before are passed in as digests, so a
/// file whose digest is in the set is allowed; a changed file has a new digest.
#[derive(Debug, Clone)]
pub struct PathPolicy {
    workspaces_root: PathBuf,
    approved: HashSet<String>,
    max_bytes: u64,
}

impl PathPolicy {
    /// `workspaces_root` must exist. It is canonicalized so both sides of the
    /// containment check are resolved the same way.
    pub fn new(workspaces_root: &Path, max_bytes: u64) -> Result<Self, MailError> {
        if max_bytes == 0 || max_bytes > HARD_MAX_ATTACHMENT_BYTES {
            return Err(MailError::InvalidInput(Redacted::new(format!(
                "the attachment limit must be between 1 and {HARD_MAX_ATTACHMENT_BYTES} bytes"
            ))));
        }
        let workspaces_root = fs::canonicalize(workspaces_root).map_err(|err| {
            MailError::InvalidConfig(Redacted::new(format!(
                "workspaces_root cannot be resolved: {err}"
            )))
        })?;
        Ok(Self {
            workspaces_root,
            approved: HashSet::new(),
            max_bytes,
        })
    }

    pub fn with_approved(mut self, digests: impl IntoIterator<Item = String>) -> Self {
        self.approved.extend(digests);
        self
    }

    pub fn decide(&self, requested: &Path) -> Result<PathDecision, MailError> {
        let path = fs::canonicalize(requested)
            .map_err(|_| rejected("the file does not exist or cannot be resolved"))?;
        if is_device(&path) {
            return Err(rejected("device paths are never attached"));
        }
        let inside = path.starts_with(&self.workspaces_root);
        if !inside && is_unc(&path) {
            return Err(rejected("UNC paths are not authorized"));
        }
        if !inside && crosses_link(requested) {
            return Err(rejected(
                "a symbolic link or junction leaves the workspaces root",
            ));
        }
        let file = self.check_file(path)?;
        let digest = approval_digest(&file);
        if inside || self.approved.contains(&digest) {
            Ok(PathDecision::Allowed(file))
        } else {
            Ok(PathDecision::NeedsApproval(PathApproval { digest, file }))
        }
    }

    /// Size and kind first, so a huge or special file is refused without being read.
    fn check_file(&self, path: PathBuf) -> Result<CheckedFile, MailError> {
        let metadata = fs::metadata(&path).map_err(unreadable)?;
        if !metadata.is_file() {
            return Err(rejected("only regular files can be attached"));
        }
        let size = metadata.len();
        if size == 0 {
            return Err(rejected("empty files are not attached"));
        }
        if size > self.max_bytes {
            return Err(rejected(format!(
                "{size} bytes is over the {} byte limit",
                self.max_bytes
            )));
        }
        let sha256 = sha256_file(&path)?;
        Ok(CheckedFile { path, size, sha256 })
    }
}

fn rejected(reason: impl Into<String>) -> MailError {
    MailError::PathRejected(Redacted::new(reason))
}

/// A local read that failed, as a refused path: the file cannot be attached.
pub(crate) fn unreadable(err: io::Error) -> MailError {
    rejected(format!("the file cannot be read: {err}"))
}

/// The Win32 device namespace: `\\.\NUL`, `\\.\PhysicalDrive0`.
fn is_device(path: &Path) -> bool {
    path.to_string_lossy().starts_with(r"\\.\")
}

fn is_unc(path: &Path) -> bool {
    path.to_string_lossy().starts_with(r"\\?\UNC\")
}

/// True when the requested path, or a directory on the way to it, is a
/// symbolic link or junction. It looks at the path as written: the canonical
/// path has already followed the link and no longer shows it.
fn crosses_link(requested: &Path) -> bool {
    let Ok(absolute) = std::path::absolute(requested) else {
        return true;
    };
    absolute.ancestors().any(|ancestor| {
        fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink())
    })
}

fn sha256_file(path: &Path) -> Result<String, MailError> {
    let mut file = File::open(path).map_err(unreadable)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(unreadable)?;
        if read == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

/// SHA-256 of the canonical JSON an approval is bound to. The keys are written
/// in sorted order, so the digest does not depend on how serde_json orders maps.
fn approval_digest(file: &CheckedFile) -> String {
    let canonical = json!({
        "accion": ACTION,
        "ruta": normalized(&file.path),
        "sha256": file.sha256,
        "tamano": file.size,
    });
    format!("{:x}", Sha256::digest(canonical.to_string()))
}

/// The path as a stable string: no verbatim prefix, forward slashes, lowercase.
fn normalized(path: &Path) -> String {
    let lossy = path.to_string_lossy();
    let text = lossy.strip_prefix(r"\\?\").unwrap_or(&lossy);
    text.replace('\\', "/").to_lowercase()
}
