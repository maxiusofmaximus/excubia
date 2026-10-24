//! First-run seeding of the host's `agents.toml`.
//!
//! The host would seed bare command names (its own default); on Windows a bare
//! `opencode` may resolve to a `.cmd` shim that `CreateProcess` cannot run.
//! Before the host starts, the app writes the entries whose executables it can
//! actually resolve, wrapping `.cmd`/`.bat` with `cmd.exe /c`. An existing file
//! is the operator's and is never touched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub const KNOWN_AGENTS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];

#[derive(Debug, PartialEq, Eq)]
pub enum SeedOutcome {
    AlreadyPresent,
    Seeded(Vec<String>),
    NoneFound,
}

#[derive(Serialize)]
struct CatalogFile {
    agents: BTreeMap<String, AgentEntry>,
}

#[derive(Serialize)]
struct AgentEntry {
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

/// PATH entries and executable extensions for this platform.
pub fn env_search() -> (Vec<PathBuf>, Vec<String>) {
    let search = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let extensions = if cfg!(windows) {
        vec![
            String::new(),
            ".exe".to_string(),
            ".cmd".to_string(),
            ".bat".to_string(),
        ]
    } else {
        vec![String::new()]
    };
    (search, extensions)
}

/// Resolves an absolute path or a bare command found on `search`.
pub fn resolve_program(name: &str, search: &[PathBuf], extensions: &[&str]) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() || name.contains(['/', '\\']) {
        return path.is_file().then(|| path.to_path_buf());
    }
    for directory in search {
        for extension in extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Writes the first catalogue, never replacing an existing one.
pub fn seed(
    data_dir: &Path,
    search: &[PathBuf],
    extensions: &[&str],
) -> Result<SeedOutcome, String> {
    let path = data_dir.join("agents.toml");
    if path.exists() {
        return Ok(SeedOutcome::AlreadyPresent);
    }

    let mut agents = BTreeMap::new();
    for name in KNOWN_AGENTS {
        if let Some(program) = resolve_program(name, search, extensions) {
            agents.insert(name.to_string(), entry_for(&program));
        }
    }
    if agents.is_empty() {
        return Ok(SeedOutcome::NoneFound);
    }

    let names: Vec<String> = agents.keys().cloned().collect();
    std::fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let body = toml::to_string(&CatalogFile { agents }).map_err(|error| error.to_string())?;
    let temporary = data_dir.join("agents.toml.tmp");
    std::fs::write(&temporary, body).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    Ok(SeedOutcome::Seeded(names))
}

/// Startup hook: seeding failures are logged, never fatal.
pub fn seed_for_env(data_dir: &Path) {
    let (search, extensions) = env_search();
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
    match seed(data_dir, &search, &extensions) {
        Ok(outcome) => eprintln!("agents.toml: {outcome:?}"),
        Err(error) => eprintln!("no se pudo sembrar agents.toml: {error}"),
    }
}

fn entry_for(program: &Path) -> AgentEntry {
    let display = program.display().to_string().replace('\\', "/");
    let lower = display.to_ascii_lowercase();
    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        // CreateProcess cannot run a batch file directly.
        AgentEntry {
            command: "cmd.exe".to_string(),
            args: vec!["/c".to_string(), display],
        }
    } else {
        AgentEntry {
            command: display,
            args: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hn-seed-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seeds_only_the_programs_that_exist_and_wraps_batch_files() {
        let bin = temp_dir("bin");
        std::fs::write(bin.join("claude.exe"), b"").unwrap();
        std::fs::write(bin.join("opencode.cmd"), b"").unwrap();
        let data = temp_dir("data");
        let search = vec![bin.clone()];
        let extensions = ["", ".exe", ".cmd"];

        let outcome = seed(&data, &search, &extensions).unwrap();
        assert_eq!(
            outcome,
            SeedOutcome::Seeded(vec!["claude".into(), "opencode".into()])
        );

        let text = std::fs::read_to_string(data.join("agents.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&text).unwrap();
        let agents = parsed.get("agents").unwrap().as_table().unwrap();
        assert!(agents.contains_key("claude"));
        assert!(!agents.contains_key("codex"), "not installed, not seeded");
        let opencode = agents.get("opencode").unwrap().as_table().unwrap();
        assert_eq!(opencode.get("command").unwrap().as_str(), Some("cmd.exe"));
        let args = opencode.get("args").unwrap().as_array().unwrap();
        assert_eq!(args[0].as_str(), Some("/c"));
        assert!(args[1].as_str().unwrap().ends_with("opencode.cmd"));
        let claude = agents.get("claude").unwrap().as_table().unwrap();
        assert!(claude
            .get("command")
            .unwrap()
            .as_str()
            .unwrap()
            .ends_with("claude.exe"));
    }

    #[test]
    fn an_existing_catalogue_is_never_overwritten() {
        let data = temp_dir("existing");
        let path = data.join("agents.toml");
        std::fs::write(&path, "[agents.mine]\ncommand = \"mine\"\n").unwrap();
        let outcome = seed(&data, &[], &[""]).unwrap();
        assert_eq!(outcome, SeedOutcome::AlreadyPresent);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[agents.mine]\ncommand = \"mine\"\n"
        );
    }

    #[test]
    fn nothing_installed_writes_nothing_and_says_so() {
        let data = temp_dir("empty");
        let outcome = seed(&data, &[], &[""]).unwrap();
        assert_eq!(outcome, SeedOutcome::NoneFound);
        assert!(!data.join("agents.toml").exists());
    }

    #[test]
    fn absolute_paths_are_checked_directly() {
        let bin = temp_dir("absolute");
        let exe = bin.join("gemini.exe");
        std::fs::write(&exe, b"").unwrap();
        assert_eq!(
            resolve_program(&exe.display().to_string(), &[], &[""]),
            Some(exe.clone())
        );
        assert!(resolve_program("D:/nope/gemini.exe", &[], &[""]).is_none());
    }
}
