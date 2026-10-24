//! The session snapshot: what a restart restores.
//!
//! Only the panes and their last screen survive — the agents themselves are
//! processes and are not relaunched. Written atomically, debounced while the
//! host runs, and once more on shutdown.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    #[serde(default)]
    pub panes: Vec<SnapshotPane>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotPane {
    pub pane_id: String,
    pub workspace_id: String,
    pub project: String,
    pub agent: String,
    pub cwd: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub last_lines: Vec<String>,
}

impl SnapshotPane {
    /// The remembered screen, one block of text.
    pub fn text(&self) -> String {
        self.last_lines.join("\n")
    }
}

pub fn write(path: &Path, panes: Vec<SnapshotPane>) -> std::io::Result<()> {
    let snapshot = Snapshot {
        version: VERSION,
        panes,
    };
    hn_core_persist::write_json_atomic(path, &snapshot)
}

/// Read a snapshot. A missing file is an empty session; a corrupt one is
/// logged and treated the same way — a bad snapshot must not stop the host.
pub fn read(path: &Path) -> Snapshot {
    match hn_core_persist::read_json::<Snapshot>(path) {
        Ok(Some(snapshot)) if snapshot.version <= VERSION => snapshot,
        Ok(Some(snapshot)) => {
            eprintln!("host: unsupported snapshot version {}", snapshot.version);
            empty()
        }
        Ok(None) => empty(),
        Err(error) => {
            eprintln!("host: snapshot unreadable, starting empty: {error}");
            empty()
        }
    }
}

fn empty() -> Snapshot {
    Snapshot {
        version: VERSION,
        panes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    /// The scratch directory, which the caller keeps alive, and the file in it.
    fn temp_path(name: &str) -> (TestDir, std::path::PathBuf) {
        let dir = TestDir::new(&format!("snapshot-tests-{name}"));
        let path = dir.join("session.json");
        (dir, path)
    }

    #[test]
    fn a_snapshot_round_trips_and_keeps_the_text() {
        let (_dir, path) = temp_path("round-trip");
        let panes = vec![SnapshotPane {
            pane_id: "w1:p1".to_string(),
            workspace_id: "w1".to_string(),
            project: "core".to_string(),
            agent: "fake".to_string(),
            cwd: "C:\\work".to_string(),
            title: Some("mi chat".to_string()),
            last_lines: vec!["uno".to_string(), "dos".to_string()],
        }];
        write(&path, panes).expect("write");
        let snapshot = read(&path);
        assert_eq!(snapshot.panes.len(), 1);
        assert_eq!(snapshot.panes[0].text(), "uno\ndos");
        assert_eq!(snapshot.panes[0].title.as_deref(), Some("mi chat"));
    }

    #[test]
    fn a_missing_or_corrupt_snapshot_is_an_empty_session() {
        let (_dir, path) = temp_path("corrupt");
        let _ = std::fs::remove_file(&path);
        assert!(read(&path).panes.is_empty());
        std::fs::write(&path, "not json").expect("write");
        assert!(read(&path).panes.is_empty());
    }
}
