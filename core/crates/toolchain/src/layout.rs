//! Where the toolchain of one profile lives, and what its processes inherit.
//!
//! Everything sits under `<data>/toolchain`: one folder per tool with fixed names
//! (`git`, `gh`), so the PATH entries exist before anything is installed and
//! start working the moment the extraction lands. Nothing is installed globally.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use excubia_persist::write_atomic;

use crate::manifest::Distribution;

/// Written last into an installed folder: its presence says the copy was verified.
pub const MARKER: &str = ".excubia-toolchain.json";

/// What `status` says about one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Missing,
    Ready { version: String },
    /// A folder is there but it is not the pinned, verified copy.
    Broken { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    /// `data_dir` is the profile's data directory; the toolchain goes under it.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("toolchain"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tool_dir(&self, dist: &Distribution) -> PathBuf {
        self.root.join(&dist.dir)
    }

    pub fn exe(&self, dist: &Distribution) -> PathBuf {
        join_relative(&self.tool_dir(dist), &dist.exe)
    }

    /// The managed profile: Git and gh configuration for this profile only.
    pub fn profile_dir(&self) -> PathBuf {
        self.root.join("profile")
    }

    pub fn gitconfig(&self) -> PathBuf {
        self.profile_dir().join("gitconfig")
    }

    pub fn gh_config_dir(&self) -> PathBuf {
        self.profile_dir().join("gh")
    }

    /// The PATH entries of the installed tools, in the order given.
    pub fn path_dirs(&self, dists: &[Distribution]) -> Vec<PathBuf> {
        dists
            .iter()
            .filter_map(|dist| self.exe(dist).parent().map(Path::to_path_buf))
            .collect()
    }

    /// The variables the toolchain's processes get: the profile's Git and gh
    /// configuration, and no prompts that would block a process with no terminal.
    pub fn profile_env(&self) -> Vec<(String, String)> {
        vec![
            (
                "GH_CONFIG_DIR".into(),
                self.gh_config_dir().to_string_lossy().into_owned(),
            ),
            (
                "GIT_CONFIG_GLOBAL".into(),
                self.gitconfig().to_string_lossy().into_owned(),
            ),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GH_PROMPT_DISABLED".into(), "1".into()),
        ]
    }

    /// Creates the profile folders and the Git file. The file includes the user's own
    /// `~/.gitconfig` (when there is one), so agents keep the usual settings. It never
    /// writes to that file.
    pub fn ensure_profile(&self) -> io::Result<()> {
        fs::create_dir_all(self.gh_config_dir())?;
        if !self.gitconfig().is_file() {
            let mut text = String::new();
            if let Some(home) = user_gitconfig() {
                text.push_str(&format!(
                    "[include]\n\tpath = {}\n",
                    home.to_string_lossy().replace('\\', "/")
                ));
            }
            write_atomic(&self.gitconfig(), text.as_bytes())?;
        }
        Ok(())
    }

    /// Startup: the PATH gets the fixed tool folders in front, and the process gets the
    /// profile variables. Installing later changes nothing here.
    pub fn apply_to_process(&self, dists: &[Distribution]) -> io::Result<()> {
        self.ensure_profile()?;
        let mut entries = self.path_dirs(dists);
        if let Some(current) = std::env::var_os("PATH") {
            for part in std::env::split_paths(&current) {
                if !entries.contains(&part) {
                    entries.push(part);
                }
            }
        }
        let joined: OsString = std::env::join_paths(entries)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        std::env::set_var("PATH", joined);
        for (key, value) in self.profile_env() {
            std::env::set_var(key, value);
        }
        Ok(())
    }

    /// Verified state of one tool.
    pub fn status(&self, dist: &Distribution) -> Status {
        let dir = self.tool_dir(dist);
        if !dir.exists() {
            return Status::Missing;
        }
        let marker = match fs::read_to_string(dir.join(MARKER)) {
            Ok(text) => text,
            Err(_) => {
                return Status::Broken {
                    reason: "sin marca de instalación verificada".into(),
                }
            }
        };
        let parsed: serde_json::Value = match serde_json::from_str(&marker) {
            Ok(value) => value,
            Err(_) => {
                return Status::Broken {
                    reason: "marca de instalación ilegible".into(),
                }
            }
        };
        if parsed["version"] != dist.version.as_str() || parsed["sha256"] != dist.sha256.as_str() {
            return Status::Broken {
                reason: "otra versión o distribución".into(),
            };
        }
        if !self.exe(dist).is_file() {
            return Status::Broken {
                reason: "falta el ejecutable".into(),
            };
        }
        Status::Ready {
            version: dist.version.clone(),
        }
    }
}

/// `base` plus a `/`-separated relative path, as native components.
pub(crate) fn join_relative(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn user_gitconfig() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let path = PathBuf::from(home).join(".gitconfig");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Distribution;
    use excubia_test_dir::TestDir;

    #[test]
    fn the_path_entries_are_the_fixed_tool_folders() {
        let dir = TestDir::new("toolchain-layout");
        let layout = Layout::new(&dir);
        let dirs = layout.path_dirs(&crate::manifest::pinned());
        assert_eq!(dirs[0], dir.join("toolchain").join("git").join("cmd"));
        assert_eq!(dirs[1], dir.join("toolchain").join("gh").join("bin"));
    }

    #[test]
    fn the_profile_env_points_at_the_managed_configuration_only() {
        let dir = TestDir::new("toolchain-env");
        let env = Layout::new(&dir).profile_env();
        let value = |key: &str| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        assert!(value("GH_CONFIG_DIR").unwrap().contains("profile"));
        assert!(value("GIT_CONFIG_GLOBAL").unwrap().ends_with("gitconfig"));
        assert!(value("GH_TOKEN").is_none());
    }

    #[test]
    fn a_folder_without_the_marker_is_broken_not_ready() {
        let dir = TestDir::new("toolchain-status");
        let layout = Layout::new(&dir);
        let dist = Distribution::gh();
        assert_eq!(layout.status(&dist), Status::Missing);
        std::fs::create_dir_all(layout.tool_dir(&dist).join("bin")).expect("dir");
        std::fs::write(layout.exe(&dist), b"x").expect("exe");
        assert!(matches!(layout.status(&dist), Status::Broken { .. }));
    }

    #[test]
    fn the_marker_must_match_the_pinned_version_and_digest() {
        let dir = TestDir::new("toolchain-marker");
        let layout = Layout::new(&dir);
        let dist = Distribution::gh();
        std::fs::create_dir_all(layout.tool_dir(&dist).join("bin")).expect("dir");
        std::fs::write(layout.exe(&dist), b"x").expect("exe");
        let stale = serde_json::json!({ "version": "2.0.0", "sha256": dist.sha256 });
        std::fs::write(layout.tool_dir(&dist).join(MARKER), stale.to_string()).expect("marker");
        assert!(matches!(layout.status(&dist), Status::Broken { .. }));
        let good = serde_json::json!({ "version": dist.version, "sha256": dist.sha256 });
        std::fs::write(layout.tool_dir(&dist).join(MARKER), good.to_string()).expect("marker");
        assert_eq!(
            layout.status(&dist),
            Status::Ready {
                version: dist.version.clone()
            }
        );
    }
}
