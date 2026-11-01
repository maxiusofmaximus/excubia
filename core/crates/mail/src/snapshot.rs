//! A private copy of an approved file, taken before anything is sent.
//!
//! The approval covers the bytes that were hashed. Uploading the original path
//! would send whatever the file holds at upload time, so the bytes are copied
//! here and hashed as they are written. A copy whose size or digest differs
//! from the approval is refused. The copy is removed when its guard drops, on
//! success and on error alike.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::error::MailError;
use crate::paths::{unreadable, CheckedFile};
use crate::private;
use crate::redact::Redacted;

const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// The private copy of an approved file. It exists only while this value does.
pub(crate) struct Snapshot {
    _removal: Removal,
    file: CheckedFile,
}

impl Snapshot {
    /// Copies `approved` into `scratch` and checks the copy against the approval.
    pub(crate) fn take(approved: &CheckedFile, scratch: &Path) -> Result<Self, MailError> {
        fs::create_dir_all(scratch).map_err(store_error)?;
        let path = scratch.join(Ulid::generate().to_string());
        let removal = Removal(path.clone());
        let (size, sha256) = copy_hashed(&approved.path, &path, approved.size)?;
        if size != approved.size || sha256 != approved.sha256 {
            return Err(MailError::ContentChanged);
        }
        Ok(Self {
            _removal: removal,
            file: CheckedFile { path, size, sha256 },
        })
    }

    /// The copy, as the upload reads it. Its path is private to this value.
    pub(crate) fn file(&self) -> &CheckedFile {
        &self.file
    }
}

/// Removes the copy when dropped. A copy that was never created is a no-op.
struct Removal(PathBuf);

impl Drop for Removal {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Copies at most one byte past `limit`, so a file that grew cannot fill the
/// scratch folder, and returns the size and digest of what was written.
fn copy_hashed(from: &Path, to: &Path, limit: u64) -> Result<(u64, String), MailError> {
    let mut source = File::open(from).map_err(unreadable)?;
    let mut target = private::create_new(to).map_err(store_error)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0; COPY_BUFFER_BYTES];
    loop {
        let read = source.read(&mut buffer).map_err(unreadable)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > limit {
            return Err(MailError::ContentChanged);
        }
        hasher.update(&buffer[..read]);
        target.write_all(&buffer[..read]).map_err(store_error)?;
    }
    target.flush().map_err(store_error)?;
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn store_error(err: std::io::Error) -> MailError {
    MailError::Store(Redacted::new(format!(
        "the private copy cannot be written: {err}"
    )))
}
