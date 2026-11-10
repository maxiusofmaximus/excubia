//! Restore points (F2-CORE): before each turn of a pane agent, the state of its
//! folder is saved, so the person can go back to it. A point is a copy with hashes,
//! kept by the host, never inside the folder and never with git. Secrets (`.env*`,
//! `*.pem`, `*.key`), `node_modules`, `target`, `.git`, symlinks and anything over
//! 50 MB are listed as left out, never copied. Commands run by the shell are not
//! traced: only the folder before and after is compared.

mod paths;
mod scan;
mod store;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use excubia_core_chat::new_ulid;
use excubia_core_persist::write_atomic;
use serde::{Deserialize, Serialize};

/// Points kept per agent; the oldest go first.
pub const POINT_LIMIT: usize = 100;
/// A point older than this goes, however few there are.
pub const POINT_MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const LIMIT_BYTES: u64 = 50 * 1024 * 1024;
/// A file whose mtime is this close to the point it was saved in may have changed
/// within the same clock tick, so its content is read again (git's "racy" rule).
const RACY_MS: u64 = 2_000;

/// The sizes past which a subfolder or a file is left out.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub dir_bytes: u64,
    pub file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            dir_bytes: LIMIT_BYTES,
            file_bytes: LIMIT_BYTES,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("no restore point with that id")]
    NotFound,
    #[error("a path in the restore point is not a plain path inside the folder")]
    InvalidPath,
    #[error("a saved copy of the point is missing")]
    MissingBlob,
    #[error("restore point i/o failed")]
    Io(#[from] std::io::Error),
}

/// Why a point is a turn's start, or the safety copy taken before an undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Turn,
    BeforeUndo,
}

/// A file in a point: its relative path, the sha256 of its content, and the size and
/// modification time (ms) it had when saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub mtime: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Name,
    TooLarge,
    Symlink,
    Special,
    UnsupportedName,
}

/// Something in the folder that the point does not keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Excluded {
    pub path: String,
    pub reason: Reason,
}

/// One saved state of a folder. `dir` stays inside the host; the API never returns it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub id: String,
    pub agent: String,
    /// The turn the point starts. For a `BeforeUndo` point it is the id of the point it precedes.
    pub turn: String,
    pub kind: Kind,
    pub created_ms: u64,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub excluded: Vec<Excluded>,
}

impl Point {
    fn index(&self) -> HashMap<&str, &Entry> {
        self.entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect()
    }

    /// Whether `entry` can vouch for a file with this metadata without reading it.
    fn trusts(&self, entry: &Entry, size: u64, mtime: u64) -> bool {
        entry.size == size && entry.mtime == mtime && entry.mtime + RACY_MS < self.created_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Added,
    Modified,
    Deleted,
}

/// A file that a restore of the point would change, with what happened to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    pub path: String,
    pub status: Status,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Changes {
    pub changes: Vec<Change>,
    pub excluded: Vec<Excluded>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Restored {
    pub restored: usize,
    pub removed: usize,
    pub undo_point: String,
}

/// The points of every agent, under one root. One lock serializes the writes.
pub struct RestoreStore {
    root: PathBuf,
    limits: Limits,
    lock: Mutex<()>,
}

impl RestoreStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_limits(root, Limits::default())
    }

    pub fn with_limits(root: impl Into<PathBuf>, limits: Limits) -> Self {
        Self {
            root: root.into(),
            limits,
            lock: Mutex::new(()),
        }
    }

    /// Saves the state of `dir` as the point at which `turn` starts.
    pub fn create_point(&self, agent: &str, turn: &str, dir: &Path) -> Result<Point, RestoreError> {
        let _guard = self.guard();
        self.create(agent, Kind::Turn, turn, dir, None)
    }

    /// The points of one agent, newest first.
    pub fn list_points(&self, agent: &str) -> Result<Vec<Point>, RestoreError> {
        store::points_of(&self.root, agent)
    }

    /// What restoring `point_id` would change: files added, modified or deleted since
    /// the point, and what the folder leaves out now.
    pub fn changed_files(&self, point_id: &str) -> Result<Changes, RestoreError> {
        let point = self.load(point_id)?;
        let now = scan::scan(&point.dir, self.limits)?;
        let saved = point.index();
        let mut changes = Vec::new();
        let mut present = HashSet::new();
        for found in &now.files {
            present.insert(found.rel.as_str());
            match saved.get(found.rel.as_str()) {
                None => changes.push(change(&found.rel, Status::Added)),
                Some(entry) if point.trusts(entry, found.size, found.mtime) => {}
                Some(entry) => {
                    if store::sha256_hex(&fs::read(&found.abs)?) != entry.sha256 {
                        changes.push(change(&found.rel, Status::Modified));
                    }
                }
            }
        }
        for entry in &point.entries {
            let gone = !present.contains(entry.path.as_str());
            if gone && !left_out(&entry.path, &now.excluded) {
                changes.push(change(&entry.path, Status::Deleted));
            }
        }
        Ok(Changes {
            changes,
            excluded: now.excluded,
        })
    }

    /// Puts the folder back as `point_id` saw it. The state before the restore is
    /// saved first as a point, so the restore can be undone with the same call.
    pub fn restore(&self, point_id: &str) -> Result<Restored, RestoreError> {
        let _guard = self.guard();
        let point = self.load(point_id)?;
        for entry in &point.entries {
            paths::safe_relative(&entry.path)?;
            if !store::has_blob(&self.root, &entry.sha256)? {
                return Err(RestoreError::MissingBlob);
            }
        }
        let before = self.create(
            &point.agent,
            Kind::BeforeUndo,
            point_id,
            &point.dir,
            Some(point_id),
        )?;
        let now = before.index();
        let mut restored = 0;
        for entry in &point.entries {
            if now
                .get(entry.path.as_str())
                .is_some_and(|c| c.sha256 == entry.sha256)
            {
                continue;
            }
            let target = paths::checked_target(&point.dir, &paths::safe_relative(&entry.path)?)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            write_atomic(&target, &store::read_blob(&self.root, &entry.sha256)?)?;
            restored += 1;
        }
        let kept: HashSet<&str> = point.entries.iter().map(|e| e.path.as_str()).collect();
        let mut removed = 0;
        for entry in &before.entries {
            if !kept.contains(entry.path.as_str()) {
                let target =
                    paths::checked_target(&point.dir, &paths::safe_relative(&entry.path)?)?;
                fs::remove_file(&target)?;
                removed += 1;
            }
        }
        Ok(Restored {
            restored,
            removed,
            undo_point: before.id,
        })
    }

    fn guard(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn load(&self, point_id: &str) -> Result<Point, RestoreError> {
        if !paths::is_point_id(point_id) {
            return Err(RestoreError::NotFound);
        }
        store::load_point(&self.root, point_id)?.ok_or(RestoreError::NotFound)
    }

    /// Saves `dir` as a new point and prunes the agent's points. Reuses the hash of an
    /// unchanged file from the agent's previous point, so only changed files are read.
    fn create(
        &self,
        agent: &str,
        kind: Kind,
        turn: &str,
        dir: &Path,
        keep: Option<&str>,
    ) -> Result<Point, RestoreError> {
        let found = scan::scan(dir, self.limits)?;
        let previous = store::points_of(&self.root, agent)?.into_iter().next();
        let known: HashMap<&str, &Entry> =
            previous.as_ref().map_or_else(HashMap::new, Point::index);
        let mut entries = Vec::with_capacity(found.files.len());
        for file in found.files {
            let sha256 = match known.get(file.rel.as_str()) {
                Some(entry)
                    if previous
                        .as_ref()
                        .is_some_and(|p| p.trusts(entry, file.size, file.mtime)) =>
                {
                    entry.sha256.clone()
                }
                _ => self.save_content(&file.abs)?,
            };
            entries.push(Entry {
                path: file.rel,
                sha256,
                size: file.size,
                mtime: file.mtime,
            });
        }
        let now = now_ms();
        let point = Point {
            id: new_ulid(),
            agent: agent.to_string(),
            turn: turn.to_string(),
            kind,
            created_ms: now,
            dir: dir.to_path_buf(),
            entries,
            excluded: found.excluded,
        };
        store::save_point(&self.root, &point)?;
        store::prune(&self.root, agent, now, keep)?;
        Ok(point)
    }

    fn save_content(&self, path: &Path) -> Result<String, RestoreError> {
        let bytes = fs::read(path)?;
        let sha256 = store::sha256_hex(&bytes);
        store::put_blob(&self.root, &sha256, &bytes)?;
        Ok(sha256)
    }
}

fn change(path: &str, status: Status) -> Change {
    Change {
        path: path.to_string(),
        status,
    }
}

/// Whether `path` is one of the left-out paths, or inside a left-out folder.
fn left_out(path: &str, excluded: &[Excluded]) -> bool {
    excluded.iter().any(|out| {
        path == out.path
            || path
                .strip_prefix(out.path.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}
