//! Paths inside a restore point: plain relative names, never through a link.

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use super::RestoreError;

/// A manifest path as a path under the folder. Only plain names: no `..`, no
/// root, no drive letter, no backslash.
pub(super) fn safe_relative(rel: &str) -> Result<PathBuf, RestoreError> {
    if rel.is_empty() || rel.contains('\\') {
        return Err(RestoreError::InvalidPath);
    }
    let path = PathBuf::from(rel);
    if path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        Ok(path)
    } else {
        Err(RestoreError::InvalidPath)
    }
}

/// Where a restored file goes. A path whose existing part crosses a link is
/// refused, so a restore never writes or removes outside the folder.
pub(super) fn checked_target(root: &Path, rel: &Path) -> Result<PathBuf, RestoreError> {
    let mut current = root.to_path_buf();
    for part in rel.components() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(RestoreError::InvalidPath),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(RestoreError::Io(error)),
        }
    }
    Ok(current)
}

/// A point id is a ULID: 26 letters and digits, so it can never be a path.
pub(super) fn is_point_id(id: &str) -> bool {
    id.len() == 26 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}
