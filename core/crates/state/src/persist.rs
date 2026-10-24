//! Persistence of the monotonic counters.
//!
//! Only `revision` and `state_change_seq` are durable: the agents themselves
//! are processes, gone with the host. What must never regress is the ordering
//! a client uses to discard stale frames, so the counters continue where the
//! previous run left them.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Counters {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub state_change_seq: u64,
}

/// Read the counters; a file that cannot be read starts at zero rather than
/// failing the host (the first frames are simply numbered from 0 again, and
/// the contract only requires monotonicity within a run).
pub fn read(path: &Path) -> Counters {
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<Counters>(&raw) {
            Ok(counters) if counters.version <= VERSION => counters,
            Ok(counters) => {
                eprintln!("state: unsupported counters version {}", counters.version);
                Counters::default()
            }
            Err(error) => {
                eprintln!("state: counters unreadable, starting at zero: {error}");
                Counters::default()
            }
        },
        Err(_) => Counters::default(),
    }
}

/// Write the counters atomically (tmp + rename), so a crash mid-write cannot
/// leave a half-written file that reads as a lower revision.
pub fn write(path: &Path, revision: u64, state_change_seq: u64) {
    let counters = Counters {
        version: VERSION,
        revision,
        state_change_seq,
    };
    if let Err(error) = hn_core_persist::write_json_atomic(path, &counters) {
        eprintln!("state: could not persist counters: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    /// The scratch directory, which the caller keeps alive, and the file in it.
    fn temp_path(name: &str) -> (TestDir, std::path::PathBuf) {
        let dir = TestDir::new(&format!("state-tests-{name}"));
        let path = dir.join("counters.json");
        (dir, path)
    }

    #[test]
    fn counters_round_trip_and_missing_files_start_at_zero() {
        let (_dir, path) = temp_path("round-trip");
        assert_eq!(read(&path).revision, 0);
        write(&path, 7, 3);
        let counters = read(&path);
        assert_eq!((counters.revision, counters.state_change_seq), (7, 3));
    }

    #[test]
    fn a_corrupt_file_starts_at_zero_instead_of_failing() {
        let (_dir, path) = temp_path("corrupt");
        fs::write(&path, "not json").expect("write");
        assert_eq!(read(&path).revision, 0);
    }
}
