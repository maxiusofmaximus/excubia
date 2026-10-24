//! A scratch directory under the system temp dir that removes itself.
//!
//! Test helpers that need a directory take a `TestDir` instead of a bare path:
//! the directory is created empty, with a name unique to the process and the
//! call, and `Drop` removes it, panics included. Nothing else is needed, so the
//! crate has no dependencies and any crate can use it as a dev-dependency.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    /// A new empty directory named `hn-<label>-<pid>-<n>` in the temp dir.
    pub fn new(label: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("hn-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("test directory");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if fs::remove_dir_all(&self.path).is_ok() || !self.path.exists() {
            return;
        }
        // A test host keeps its database open until its tokio runtime is dropped,
        // and that happens after this guard: retry from a thread that outlives
        // the test body. A file still open when the process exits stays behind.
        let path = self.path.clone();
        thread::spawn(move || {
            for _ in 0..100 {
                thread::sleep(Duration::from_millis(100));
                if fs::remove_dir_all(&path).is_ok() || !path.exists() {
                    return;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::TestDir;

    #[test]
    fn a_directory_exists_while_held_and_is_removed_on_drop() {
        let path = {
            let dir = TestDir::new("guard-drop");
            assert!(dir.is_dir());
            dir.path().to_path_buf()
        };
        assert!(!path.exists(), "dropped directory must be gone");
    }

    #[test]
    fn two_directories_with_the_same_label_do_not_collide() {
        let first = TestDir::new("guard-same");
        let second = TestDir::new("guard-same");
        assert_ne!(first.path(), second.path());
    }
}
