//! The folder as a restore point sees it: which files are kept, and which are left
//! out and why. Only metadata is read here; contents are read by the caller.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{Excluded, Limits, Reason};

/// A file the point keeps, with what its metadata says.
pub(super) struct Found {
    pub(super) rel: String,
    pub(super) abs: PathBuf,
    pub(super) size: u64,
    pub(super) mtime: u64,
}

#[derive(Default)]
pub(super) struct Scan {
    pub(super) files: Vec<Found>,
    pub(super) excluded: Vec<Excluded>,
}

/// Walks the folder. The folder itself has no size limit: only its subfolders
/// and its files do, so a big project still gets its points.
pub(super) fn scan(root: &Path, limits: Limits) -> io::Result<Scan> {
    let mut out = Scan::default();
    walk(root, "", u64::MAX, limits, &mut out)?;
    Ok(out)
}

/// Adds the files of `dir` to `out` and returns the bytes they hold. Stops counting
/// once the bytes pass `cap`: the caller then drops the whole subfolder anyway.
fn walk(dir: &Path, prefix: &str, cap: u64, limits: Limits, out: &mut Scan) -> io::Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            out.excluded.push(Excluded {
                path: join(prefix, "(nombre no UTF-8)"),
                reason: Reason::UnsupportedName,
            });
            continue;
        };
        let rel = join(prefix, &name);
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            out.excluded.push(excluded(rel, Reason::Symlink));
        } else if is_excluded_name(&name, kind.is_dir()) {
            out.excluded.push(excluded(rel, Reason::Name));
        } else if kind.is_dir() {
            let mut sub = Scan::default();
            let bytes = walk(&entry.path(), &rel, limits.dir_bytes, limits, &mut sub)?;
            if bytes > limits.dir_bytes {
                out.excluded.push(excluded(rel, Reason::TooLarge));
            } else {
                total += bytes;
                out.files.append(&mut sub.files);
                out.excluded.append(&mut sub.excluded);
            }
        } else if kind.is_file() {
            let meta = entry.metadata()?;
            if meta.len() > limits.file_bytes {
                out.excluded.push(excluded(rel, Reason::TooLarge));
            } else {
                total += meta.len();
                out.files.push(Found {
                    rel,
                    abs: entry.path(),
                    size: meta.len(),
                    mtime: modified_ms(&meta),
                });
            }
        } else {
            out.excluded.push(excluded(rel, Reason::Special));
        }
        if total > cap {
            break;
        }
    }
    Ok(total)
}

/// The rules that keep secrets and build output out of a point, by name.
fn is_excluded_name(name: &str, is_dir: bool) -> bool {
    if is_dir {
        matches!(name, "node_modules" | "target" | ".git")
    } else {
        name.starts_with(".env") || name.ends_with(".pem") || name.ends_with(".key")
    }
}

fn excluded(path: String, reason: Reason) -> Excluded {
    Excluded { path, reason }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn modified_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as u64)
}
