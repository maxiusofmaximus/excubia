//! Files only the owner can read. On Unix they are created with mode 0600; on
//! other platforms they inherit the accounts folder's ACL, which is the user's
//! profile.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Opens `path` for appending, creating it owner-only when it is new.
pub(crate) fn open_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    owner_only(&mut options);
    options.open(path)
}

/// Creates a file that must not exist yet, owner-only.
pub(crate) fn create_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    owner_only(&mut options);
    options.open(path)
}

#[cfg(unix)]
fn owner_only(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn owner_only(_options: &mut OpenOptions) {}
