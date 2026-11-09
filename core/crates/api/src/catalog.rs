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
    /// Extra environment for the agent's process. Names match `^[A-Z][A-Z0-9_]*$`
    /// and secret-looking names are refused, so no key is ever written here.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Credential-looking names a local backend may still carry, with the one
/// value that makes them harmless: a fixed placeholder the local server
/// does not check (`strata connect claude-code` writes the same one).
const PLACEHOLDER_CREDENTIALS: [(&str, &str); 1] = [("ANTHROPIC_AUTH_TOKEN", "strata-local")];

/// Names that contain a credential word but hold a count, not a credential.
const NOT_CREDENTIALS: [&str; 2] = [
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
];

/// Refuse an `env` entry that could carry a secret or break the child's environment.
fn check_env_entry(key: &str, value: &str) -> Result<(), String> {
    let well_formed = key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !well_formed {
        return Err("the name must match ^[A-Z][A-Z0-9_]*$".to_string());
    }
    if value.contains(['\n', '\r']) {
        return Err("the value has a line break".to_string());
    }
    let secret_like = ["KEY", "TOKEN", "SECRET", "PASSWORD"]
        .iter()
        .any(|word| key.contains(word));
    if secret_like
        && !NOT_CREDENTIALS.contains(&key)
        && !PLACEHOLDER_CREDENTIALS.contains(&(key, value))
    {
        return Err("it looks like a secret; keys never go in agents.toml".to_string());
    }
    Ok(())
}

impl AgentSpec {
    /// True when the command would resolve on this machine: a path that
    /// exists, or a bare name found on `PATH` (with the Windows executable
    /// extensions when applicable). The resolved path is never returned.
    pub fn available(&self) -> bool {
        self.located().is_some()
    }

    /// The file the command resolves to here, if any. Private to the host's own checks.
    fn located(&self) -> Option<PathBuf> {
        locate(
            &self.command,
            std::env::var_os("PATH"),
            std::env::var_os("PATHEXT"),
        )
    }

    /// True when the host can start the command as it stands. On Windows a file without an
    /// extension (pnpm's `sh` script) is found but cannot be started, so it is not runnable.
    pub fn runnable(&self) -> bool {
        self.located().is_some_and(|file| runnable_file(&file))
    }

    /// Windows: the `.CMD` shim beside the extensionless script the command resolves to, the one
    /// `cmd.exe /c` can run. None when the command is runnable or there is no shim to use.
    pub fn cmd_shim(&self) -> Option<PathBuf> {
        let file = self.located()?;
        if runnable_file(&file) {
            return None;
        }
        let mut shim = file.into_os_string();
        shim.push(".CMD");
        let shim = PathBuf::from(shim);
        shim.is_file().then_some(shim)
    }
}

/// Whether the host can start this file directly. On Windows only a file with an extension
/// (`.exe`, `.cmd`, …) can be started. A `.exe` that is not a program is not caught here: the start fails.
fn runnable_file(file: &Path) -> bool {
    !cfg!(windows) || file.extension().is_some()
}

/// One catalogue entry as the API publishes it.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub name: String,
    pub label: Option<String>,
    /// The command resolves on this machine.
    pub available: bool,
    /// The host can start it as it stands.
    pub runnable: bool,
    /// `fix` can make it runnable (a `.CMD` shim beside it, Windows).
    pub fixable: bool,
}

/// What `AgentCatalog::fix` did.
#[derive(Debug, PartialEq, Eq)]
pub enum FixOutcome {
    /// The entry was already runnable; nothing was written.
    AlreadyRunnable,
    /// The entry now runs through `cmd.exe /c <shim>`; the backup is beside `agents.toml`.
    Fixed,
}

#[derive(Debug)]
pub enum FixError {
    UnknownAgent,
    /// Not runnable, and no `.CMD` shim beside the command to point at.
    NotFixable,
    /// Read, backup or write failed, or the new file would not parse. Nothing changed.
    Write(String),
}

/// The file beside `agents.toml` that names this PC's default agent.
const DEFAULT_FILE: &str = "default-agent.txt";

/// What `AgentCatalog::set_default` refused.
#[derive(Debug)]
pub enum DefaultError {
    UnknownAgent,
    /// The entry is not runnable here, so it cannot be the default.
    NotRunnable,
    /// The file could not be written. Nothing changed.
    Write(String),
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
                runnable: spec.runnable(),
                fixable: spec.cmd_shim().is_some(),
            })
            .collect()
    }

    /// Point a non-runnable entry at its `.CMD` shim through `cmd.exe /c`. The file is backed up
    /// beside itself first, and written only if the result still parses; the catalogue then reloads.
    pub fn fix(&self, name: &str) -> Result<FixOutcome, FixError> {
        self.reload_if_changed();
        let spec = self.resolve(name).ok_or(FixError::UnknownAgent)?;
        if spec.runnable() {
            return Ok(FixOutcome::AlreadyRunnable);
        }
        let shim = spec.cmd_shim().ok_or(FixError::NotFixable)?;
        let path = self
            .source
            .as_ref()
            .ok_or_else(|| FixError::Write("the catalogue has no file".to_string()))?;
        let text =
            std::fs::read_to_string(path).map_err(|error| FixError::Write(error.to_string()))?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|error: toml_edit::TomlError| FixError::Write(error.to_string()))?;
        let entry = doc
            .get_mut("agents")
            .and_then(|agents| agents.as_table_mut())
            .and_then(|agents| agents.get_mut(name))
            .and_then(|entry| entry.as_table_mut())
            .ok_or_else(|| FixError::Write(format!("agent {name:?} is not a table")))?;
        let mut args = toml_edit::Array::new();
        args.push("/c");
        args.push(shim.to_string_lossy().into_owned());
        entry["command"] = toml_edit::value("cmd.exe");
        entry["args"] = toml_edit::value(args);
        let new_text = doc.to_string();
        parse(&new_text).map_err(FixError::Write)?;

        let stamp_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let backup = path.with_file_name(format!("{file_name}.bak-{stamp_secs}"));
        std::fs::copy(path, &backup).map_err(|error| FixError::Write(error.to_string()))?;
        let temporary = path.with_file_name(format!("{file_name}.tmp"));
        std::fs::write(&temporary, &new_text)
            .map_err(|error| FixError::Write(error.to_string()))?;
        std::fs::rename(&temporary, path).map_err(|error| FixError::Write(error.to_string()))?;

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.agents = parse(&new_text).map_err(FixError::Write)?;
        state.stamp = stamp_of(path);
        state.error = None;
        Ok(FixOutcome::Fixed)
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

    /// The agent this PC starts new conversations with, while it still exists and runs.
    pub fn default_name(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.default_path()?).ok()?;
        let name = text.trim();
        self.resolve(name)?.runnable().then(|| name.to_string())
    }

    /// Remember `name` as this PC's default agent, in a file beside `agents.toml`.
    pub fn set_default(&self, name: &str) -> Result<(), DefaultError> {
        self.reload_if_changed();
        let spec = self.resolve(name).ok_or(DefaultError::UnknownAgent)?;
        if !spec.runnable() {
            return Err(DefaultError::NotRunnable);
        }
        let path = self
            .default_path()
            .ok_or_else(|| DefaultError::Write("the catalogue has no file".to_string()))?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, name).map_err(|error| DefaultError::Write(error.to_string()))?;
        std::fs::rename(&temporary, &path).map_err(|error| DefaultError::Write(error.to_string()))
    }

    fn default_path(&self) -> Option<PathBuf> {
        self.source
            .as_ref()
            .map(|path| path.with_file_name(DEFAULT_FILE))
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
        for (key, value) in &spec.env {
            check_env_entry(key, value)
                .map_err(|reason| format!("agent {name:?} env {key:?}: {reason}"))?;
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
#[cfg(test)]
fn resolves(program: &str, path: Option<OsString>, pathext: Option<OsString>) -> bool {
    locate(program, path, pathext).is_some()
}

/// The first existing file a command would refer to, in the order the host tries them.
fn locate(program: &str, path: Option<OsString>, pathext: Option<OsString>) -> Option<PathBuf> {
    let program = program.trim();
    if program.is_empty() {
        return None;
    }
    let path_like = program.contains('/') || program.contains('\\');
    if path_like {
        return candidates(Path::new(program), &pathext)
            .into_iter()
            .find(|file| file.is_file());
    }
    let path_var = path?;
    std::env::split_paths(&path_var).find_map(|directory| {
        candidates(&directory.join(program), &pathext)
            .into_iter()
            .find(|file| file.is_file())
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
    fn an_agent_carries_its_environment_and_no_key() {
        let catalog = AgentCatalog::from_toml(
            r#"
[agents.claude-local]
command = "claude"
env = { ANTHROPIC_BASE_URL = "http://127.0.0.1:8080", ANTHROPIC_AUTH_TOKEN = "strata-local" }
"#,
        )
        .expect("catalog");
        let env = catalog.resolve("claude-local").expect("spec").env;
        assert!(AgentCatalog::from_toml(
            "[agents.x]\ncommand = 'x'\nenv = { CLAUDE_CODE_MAX_CONTEXT_TOKENS = \"65536\" }\n"
        )
        .is_ok());
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("http://127.0.0.1:8080")
        );

        for broken in [
            "[agents.x]\ncommand = 'x'\nenv = { lower = \"1\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { \"1BAD\" = \"1\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { A-B = \"1\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { MODEL = \"line\\nbreak\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { ANTHROPIC_API_KEY = \"sk-real\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { MY_SECRET = \"1\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { PGPASSWORD = \"1\" }\n",
            "[agents.x]\ncommand = 'x'\nenv = { ANTHROPIC_AUTH_TOKEN = \"sk-real\" }\n",
        ] {
            assert!(
                AgentCatalog::from_toml(broken).is_err(),
                "accepted: {broken}"
            );
        }
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
            env: BTreeMap::new(),
        };
        assert!(spec.available());
        let spec = AgentSpec {
            command: "hn-no-such-binary-anywhere".to_string(),
            args: Vec::new(),
            label: None,
            env: BTreeMap::new(),
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

    #[test]
    #[cfg(windows)]
    fn an_extensionless_script_is_not_runnable_and_gets_its_cmd_shim() {
        let dir = temp_dir("shim");
        let script = dir.join("tool");
        let shim = dir.join("tool.CMD");
        std::fs::write(&script, b"#!/bin/sh").expect("write");
        std::fs::write(&shim, b"@echo off").expect("write");
        let spec = AgentSpec {
            command: script.to_string_lossy().to_string(),
            args: Vec::new(),
            label: None,
            env: BTreeMap::new(),
        };
        assert!(spec.available(), "the script is found");
        assert!(!spec.runnable(), "but Windows cannot start it");
        assert_eq!(spec.cmd_shim(), Some(shim));
    }

    #[test]
    fn fix_points_the_entry_at_its_shim_and_keeps_a_backup() {
        let dir = temp_dir("fix");
        let path = dir.join("agents.toml");
        let script = dir.join("tool");
        std::fs::write(&script, b"#!/bin/sh").expect("write");
        std::fs::write(dir.join("tool.CMD"), b"@echo off").expect("write");
        let text = format!(
            "[agents.tool]\ncommand = '{}'\nargs = []\nlabel = \"Tool\"\n",
            script.display()
        );
        std::fs::write(&path, &text).expect("write");
        let catalog = AgentCatalog::open(path.clone()).expect("open");

        assert!(matches!(catalog.fix("nope"), Err(FixError::UnknownAgent)));
        if cfg!(windows) {
            assert_eq!(catalog.fix("tool").expect("fixed"), FixOutcome::Fixed);
            let spec = catalog.resolve("tool").expect("spec");
            assert_eq!(spec.command, "cmd.exe");
            assert_eq!(spec.args.first().map(String::as_str), Some("/c"));
            assert_eq!(spec.label.as_deref(), Some("Tool"));
            let backups = std::fs::read_dir(&dir)
                .expect("dir")
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().contains(".bak-"))
                .count();
            assert_eq!(backups, 1, "the file is backed up before it changes");
        } else {
            assert!(matches!(catalog.fix("tool"), Err(FixError::NotFixable)));
            assert_eq!(std::fs::read_to_string(&path).expect("read"), text);
        }
    }

    #[test]
    fn the_default_agent_is_remembered_only_while_it_runs() {
        let dir = temp_dir("default");
        let path = dir.join("agents.toml");
        let present = dir.join("present.bin");
        std::fs::write(&present, b"stub").expect("write");
        let text = format!(
            "[agents.here]\ncommand = '{}'\n\n[agents.gone]\ncommand = 'hn-no-such-binary-anywhere'\n",
            present.display()
        );
        std::fs::write(&path, &text).expect("write");
        let catalog = AgentCatalog::open(path).expect("open");

        assert_eq!(catalog.default_name(), None);
        assert!(matches!(
            catalog.set_default("nope"),
            Err(DefaultError::UnknownAgent)
        ));
        assert!(matches!(
            catalog.set_default("gone"),
            Err(DefaultError::NotRunnable)
        ));
        catalog.set_default("here").expect("set");
        assert_eq!(catalog.default_name().as_deref(), Some("here"));

        // A default that stops running is not offered.
        std::fs::remove_file(&present).expect("remove");
        assert_eq!(catalog.default_name(), None);
    }
}
