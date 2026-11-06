//! What a restore point keeps on disk: one manifest per point (`points/<id>.json`)
//! and one blob per distinct content (`blobs/<sha256>`), shared by every point that
//! holds that content.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use hn_core_persist::{read_json, write_atomic, write_json_atomic};
use sha2::{Digest, Sha256};

use super::{Point, RestoreError, POINT_LIMIT, POINT_MAX_AGE_MS};

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn points_dir(root: &Path) -> PathBuf {
    root.join("points")
}

fn blobs_dir(root: &Path) -> PathBuf {
    root.join("blobs")
}

/// A blob name is a sha256 in hex, read from a manifest: anything else is refused
/// before it reaches the file system.
fn blob_path(root: &Path, sha256: &str) -> Result<PathBuf, RestoreError> {
    let plain = sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    if plain {
        Ok(blobs_dir(root).join(sha256))
    } else {
        Err(RestoreError::InvalidPath)
    }
}

pub(super) fn save_point(root: &Path, point: &Point) -> Result<(), RestoreError> {
    fs::create_dir_all(points_dir(root))?;
    write_json_atomic(&points_dir(root).join(format!("{}.json", point.id)), point)?;
    Ok(())
}

pub(super) fn load_point(root: &Path, id: &str) -> Result<Option<Point>, RestoreError> {
    Ok(read_json(&points_dir(root).join(format!("{id}.json")))?)
}

/// Every readable point of every agent. A manifest that does not parse is skipped:
/// one damaged file must not stop the turns of the agents.
fn all_points(root: &Path) -> Result<Vec<Point>, RestoreError> {
    let dir = match fs::read_dir(points_dir(root)) {
        Ok(dir) => dir,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut points = Vec::new();
    for entry in dir {
        let path = entry?.path();
        match read_json::<Point>(&path) {
            Ok(Some(point)) => points.push(point),
            _ => eprintln!("host: skipped an unreadable restore point file"),
        }
    }
    Ok(points)
}

/// The points of one agent, newest first.
pub(super) fn points_of(root: &Path, agent: &str) -> Result<Vec<Point>, RestoreError> {
    let mut points: Vec<Point> = all_points(root)?
        .into_iter()
        .filter(|point| point.agent == agent)
        .collect();
    points.sort_by(|a, b| (b.created_ms, &b.id).cmp(&(a.created_ms, &a.id)));
    Ok(points)
}

pub(super) fn put_blob(root: &Path, sha256: &str, bytes: &[u8]) -> Result<(), RestoreError> {
    let path = blob_path(root, sha256)?;
    if !path.is_file() {
        fs::create_dir_all(blobs_dir(root))?;
        write_atomic(&path, bytes)?;
    }
    Ok(())
}

pub(super) fn has_blob(root: &Path, sha256: &str) -> Result<bool, RestoreError> {
    Ok(blob_path(root, sha256)?.is_file())
}

pub(super) fn read_blob(root: &Path, sha256: &str) -> Result<Vec<u8>, RestoreError> {
    match fs::read(blob_path(root, sha256)?) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(RestoreError::MissingBlob),
        Err(error) => Err(error.into()),
    }
}

/// Keeps the newest `POINT_LIMIT` points of the agent and the ones younger than
/// `POINT_MAX_AGE_MS`; the newest point always stays, and so does `keep` (the point a
/// restore is reading from). Then drops the blobs no point holds any more.
pub(super) fn prune(
    root: &Path,
    agent: &str,
    now_ms: u64,
    keep: Option<&str>,
) -> Result<(), RestoreError> {
    let points = points_of(root, agent)?;
    for (index, point) in points.iter().enumerate() {
        let too_old = now_ms.saturating_sub(point.created_ms) > POINT_MAX_AGE_MS;
        let expired = index >= POINT_LIMIT || (index > 0 && too_old);
        if expired && keep != Some(point.id.as_str()) {
            fs::remove_file(points_dir(root).join(format!("{}.json", point.id)))?;
        }
    }
    collect_blobs(root)
}

fn collect_blobs(root: &Path) -> Result<(), RestoreError> {
    let referenced: HashSet<String> = all_points(root)?
        .into_iter()
        .flat_map(|point| point.entries.into_iter().map(|entry| entry.sha256))
        .collect();
    let dir = match fs::read_dir(blobs_dir(root)) {
        Ok(dir) => dir,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in dir {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !referenced.contains(&name) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
