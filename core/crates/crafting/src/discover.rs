//! Where the CLIs are: the path in `crafting.json`, then the host's tools folder,
//! then `PATH`. Discovery never runs a CLI.

use crate::{App, CraftError};
use std::collections::HashMap;
use std::env::{self, consts::EXE_SUFFIX};
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "crafting.json";

/// The executables found for each app; an app without an entry is unavailable.
#[derive(Debug)]
pub struct Tools {
    paths: HashMap<App, PathBuf>,
}

impl Tools {
    /// Reads `crafting.json` from `config_dir`, then searches `tools_dir` (one folder per
    /// app), then the `PATH` given as `path_var`.
    pub fn discover(
        config_dir: &Path,
        tools_dir: &Path,
        path_var: Option<&OsStr>,
    ) -> Result<Self, CraftError> {
        let config = read_config(&config_dir.join(CONFIG_FILE))?;
        let paths = App::ALL
            .into_iter()
            .filter_map(|app| locate(app, tools_dir, &config, path_var).map(|path| (app, path)))
            .collect();
        Ok(Self { paths })
    }

    /// No CLI at all: every app is missing.
    pub fn none() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }

    pub fn path(&self, app: App) -> Option<&Path> {
        self.paths.get(&app).map(PathBuf::as_path)
    }

    pub fn available(&self, app: App) -> bool {
        self.paths.contains_key(&app)
    }

    pub(crate) fn require(&self, app: App) -> Result<&Path, CraftError> {
        self.path(app).ok_or(CraftError::ToolMissing(app))
    }
}

fn read_config(path: &Path) -> Result<HashMap<String, PathBuf>, CraftError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| CraftError::BadConfig),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(HashMap::new()),
        Err(err) => Err(err.into()),
    }
}

fn locate(
    app: App,
    tools_dir: &Path,
    config: &HashMap<String, PathBuf>,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    let exe = format!("{}{EXE_SUFFIX}", app.cli());
    config
        .get(app.config_key())
        .filter(|path| path.is_file())
        .cloned()
        .or_else(|| in_tool_folder(&tools_dir.join(app.dir()), &exe))
        .or_else(|| path_var.and_then(|dirs| on_path(dirs, &exe)))
}

/// The executable directly in `dir`, or one folder down (where a portable zip unpacks).
fn in_tool_folder(dir: &Path, exe: &str) -> Option<PathBuf> {
    let direct = dir.join(exe);
    if direct.is_file() {
        return Some(direct);
    }
    let mut folders: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    folders
        .iter()
        .map(|folder| folder.join(exe))
        .find(|path| path.is_file())
}

fn on_path(dirs: &OsStr, exe: &str) -> Option<PathBuf> {
    env::split_paths(dirs)
        .map(|dir| dir.join(exe))
        .find(|path| path.is_file())
}
