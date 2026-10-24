//! The closed list of agents the client may start.
//!
//! `POST /v1/workspaces` names an agent from this list; the executable and
//! its arguments come from the host's own file (`agents.toml`), never from
//! the request. `GET /v1/catalog` publishes the names and whether each one
//! would resolve on this machine — never the command, its arguments or a
//! resolved path.
//!
//! The file is re-read when its mtime changes, so adding, removing or editing
//! entries does not need a host restart. A file that stops parsing keeps the
//! last good catalogue: the host does not fall over, and the endpoints report
//! the error.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Deserialize;

/// One agent the host knows how to start.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// A human label for clients; the name is the fallback.
    #[serde(default)]
    pub label: Option<String>,
}

impl AgentSpec {
    /// True when the command would resolve on this machine: a path that
    /// exists, or a bare name found on `PATH` (with the Windows executable
    /// extensions when applicable). The resolved path is never returned.
    pub fn available(&self) -> bool {
        resolves(
            &self.command,
            std::env::var_os("PATH"),
            std::env::var_os("PATHEXT"),
        )
    }
}

/// One catalogue entry as the API publishes it.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub name: String,
    pub label: Option<String>,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    #[serde(default)]
    agents: BTreeMap<String, AgentSpec>,
}

#[derive(Debug, Default)]
struct CatalogState {
    agents: BTreeMap<String, AgentSpec>,
    stamp: Option<SystemTime>,
    /// The last reload failure, while the file is still broken.
    error: Option<String>,
}

/// The catalogue, reloaded from its file when the file changes.
pub struct AgentCatalog {
    source: Option<PathBuf>,
    state: Mutex<CatalogState>,
}

impl AgentCatalog {
    /// A catalogue with no file behind it (tests, in-memory callers).
    pub fn from_toml(text: &str) -> Result<Self, String> {
        Ok(Self {
            source: None,
            state: Mutex::new(CatalogState {
                agents: parse(text)?,
                stamp: None,
                error: None,
            }),
        })
    }

    /// Open a catalogue file. A file that is already invalid is a startup
    /// error: there is no last-good copy to keep yet.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let agents = parse(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        let stamp = stamp_of(&path);
        Ok(Self {
            source: Some(path),
            state: Mutex::new(CatalogState {
                agents,
                stamp,
                error: None,
            }),
        })
    }

    /// Re-read the file when another process changed it. A file that no
    /// longer parses keeps the last good catalogue and records the error.
    pub fn reload_if_changed(&self) {
        let Some(path) = &self.source else {
            return;
        };
        let stamp = stamp_of(path);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if stamp == state.stamp {
            return;
        }
        let outcome = std::fs::read_to_string(path)
            .map_err(|error| error.to_string())
            .and_then(|text| parse(&text));
        match outcome {
            Ok(agents) => {
                state.agents = agents;
                state.error = None;
            }
            Err(error) => {
                // Log a change of error, not every request.
                if state.error.as_deref() != Some(error.as_str()) {
                    eprintln!(
                        "host: agents.toml is invalid, keeping the last good catalogue: {error}"
                    );
                }
                state.error = Some(error);
            }
        }
        state.stamp = stamp;
    }

    /// The current reload error, while the file is broken.
    pub fn warning(&self) -> Option<String> {
        self.reload_if_changed();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.error.clone()
    }

    /// The catalogue, sorted by name, with each entry's availability.
    pub fn entries(&self) -> Vec<CatalogEntry> {
        self.reload_if_changed();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .agents
            .iter()
            .map(|(name, spec)| CatalogEntry {
                name: name.clone(),
                label: spec.label.clone(),
                available: spec.available(),
            })
            .collect()
    }

    /// One entry by name, if the catalogue has it.
    pub fn resolve(&self, name: &str) -> Option<AgentSpec> {
        self.reload_if_changed();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.agents.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.reload_if_changed();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.agents.keys().cloned().collect()
    }
}

fn parse(text: &str) -> Result<BTreeMap<String, AgentSpec>, String> {
    let file: CatalogFile = toml::from_str(text).map_err(|error| error.to_string())?;
    if file.agents.is_empty() {
        return Err("agents.toml lists no agents".to_string());
    }
    for (name, spec) in &file.agents {
        if spec.command.trim().is_empty() {
            return Err(format!("agent {name:?} has an empty command"));
        }
    }
    Ok(file.agents)
}

fn stamp_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

/// Resolve a command the way the host would start it, without running it.
/// `path` and `pathext` are the environment values, injectable for tests.
fn resolves(program: &str, path: Option<OsString>, pathext: Option<OsString>) -> bool {
    let program = program.trim();
    if program.is_empty() {
        return false;
    }
    let path_like = program.contains('/') || program.contains('\\');
    let candidate = Path::new(program);
    if path_like {
        return candidates(candidate, &pathext)
            .iter()
            .any(|file| file.is_file());
    }
    let Some(path_var) = path else {
        return false;
    };
    std::env::split_paths(&path_var).any(|directory| {
        candidates(&directory.join(program), &pathext)
            .iter()
            .any(|file| file.is_file())
    })
}

/// The files a command may refer to: itself, plus the Windows executable
/// extensions when given (`.EXE`, `.CMD`, …).
fn candidates(path: &Path, pathext: &Option<OsString>) -> Vec<PathBuf> {
    let mut files = vec![path.to_path_buf()];
    #[cfg(windows)]
    if let Some(pathext) = pathext {
        for extension in pathext.to_string_lossy().split(';') {
            let extension = extension.trim().trim_start_matches('.');
            if !extension.is_empty() {
                let mut name = path.as_os_str().to_os_string();
                name.push(".");
                name.push(extension);
                files.push(PathBuf::from(name));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = pathext;
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    fn temp_dir(name: &str) -> TestDir {
        TestDir::new(&format!("catalog-tests-{name}"))
    }

    #[test]
    fn a_catalogue_resolves_names_never_commands() {
        let catalog = AgentCatalog::from_toml(
            r#"
[agents.opencode]
command = "opencode"
args = ["--model", "deepseek"]
label = "opencode"

[agents.fake]
command = "C:\\tools\\fake-agent.exe"
"#,
        )
        .expect("catalog");
        assert_eq!(catalog.names(), vec!["fake", "opencode"]);
        let spec = catalog.resolve("opencode").expect("spec");
        assert_eq!(spec.command, "opencode");
        assert_eq!(spec.args, vec!["--model", "deepseek"]);
        assert_eq!(spec.label.as_deref(), Some("opencode"));
        assert!(catalog.resolve("rm -rf /").is_none());
        assert!(catalog.resolve("C:\\evil.exe").is_none());
    }

    #[test]
    fn an_empty_or_broken_catalogue_is_refused() {
        assert!(AgentCatalog::from_toml("").is_err());
        assert!(AgentCatalog::from_toml("[agents.x]\ncommand = \"  \"").is_err());
        assert!(AgentCatalog::from_toml("not toml").is_err());
    }

    #[test]
    fn availability_checks_paths_without_exposing_them() {
        let dir = temp_dir("availability");
        let present = dir.join("present-binary");
        std::fs::write(&present, b"stub").expect("write");
        let missing = dir.join("missing-binary");
        let path = Some(OsString::from(dir.as_os_str()));

        assert!(resolves("present-binary", path.clone(), None));
        assert!(!resolves("missing-binary", path, None));
        assert!(resolves(&present.to_string_lossy(), None, None));
        assert!(!resolves(&missing.to_string_lossy(), None, None));
        assert!(
            !resolves("present-binary", None, None),
            "no PATH, no answer"
        );
        assert!(!resolves("", Some(OsString::from(dir.as_os_str())), None));
    }

    #[test]
    fn the_public_availability_matches_the_filesystem() {
        let dir = temp_dir("public");
        let present = dir.join("present-binary");
        std::fs::write(&present, b"stub").expect("write");
        let spec = AgentSpec {
            command: present.to_string_lossy().to_string(),
            args: Vec::new(),
            label: None,
        };
        assert!(spec.available());
        let spec = AgentSpec {
            command: "hn-no-such-binary-anywhere".to_string(),
            args: Vec::new(),
            label: None,
        };
        assert!(!spec.available());
    }

    #[test]
    fn the_catalogue_reloads_by_mtime_and_keeps_the_last_good_copy() {
        let dir = temp_dir("reload");
        let path = dir.join("agents.toml");
        std::fs::write(&path, "[agents.alpha]\ncommand = 'alpha'\n").expect("write");
        let catalog = AgentCatalog::open(path.clone()).expect("open");
        assert_eq!(catalog.names(), vec!["alpha"]);

        // Add an entry.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            &path,
            "[agents.alpha]\ncommand = 'alpha'\n\n[agents.beta]\ncommand = 'beta'\n",
        )
        .expect("write");
        assert_eq!(catalog.names(), vec!["alpha", "beta"]);

        // Remove one.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "[agents.beta]\ncommand = 'beta'\n").expect("write");
        assert_eq!(catalog.names(), vec!["beta"]);

        // Break it: the last good copy stays and the error is reported.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "not toml").expect("write");
        assert_eq!(catalog.names(), vec!["beta"]);
        assert!(catalog.warning().is_some());

        // Fix it: the warning goes away.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, "[agents.gamma]\ncommand = 'gamma'\n").expect("write");
        assert_eq!(catalog.names(), vec!["gamma"]);
        assert!(catalog.warning().is_none());
    }
}
