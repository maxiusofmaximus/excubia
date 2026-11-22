//! The entry point: runs the CLIs for one chat attachment at a time.

use crate::runner::{failure_detail, Captured, Invocation, Runner};
use crate::workspace::Workspace;
use crate::{App, CraftError, Tools};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const TEXT_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const RENDER_TIMEOUT: Duration = Duration::from_secs(60);

/// A chat attachment as the host hands it over: where its bytes are, the name
/// the user gave it and the mime the host fixed.
#[derive(Debug, Clone)]
pub struct Document {
    pub path: PathBuf,
    pub name: String,
    pub mime: String,
}

/// What `--version` printed and whether it is the version the apps were tested with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReport {
    pub app: App,
    pub found: String,
    pub tested: &'static str,
    pub matches: bool,
}

/// Runs the document CLIs found in `tools`, through `runner`, on copies made
/// in folders under `base`.
pub struct Driver<'a> {
    pub(crate) tools: &'a Tools,
    pub(crate) runner: &'a dyn Runner,
    pub(crate) base: PathBuf,
}

impl<'a> Driver<'a> {
    pub fn new(tools: &'a Tools, runner: &'a dyn Runner, base: impl Into<PathBuf>) -> Self {
        Self {
            tools,
            runner,
            base: base.into(),
        }
    }

    pub fn version(&self, app: App) -> Result<VersionReport, CraftError> {
        let ws = Workspace::create(&self.base)?;
        let out = self.invoke(
            app,
            vec![OsString::from("--version")],
            ws.path(),
            TEXT_TIMEOUT,
        )?;
        let text = String::from_utf8_lossy(&out.stdout);
        let found = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_string();
        let tested = app.tested_version();
        let matches = found.split_whitespace().last() == Some(tested);
        Ok(VersionReport {
            app,
            found,
            tested,
            matches,
        })
    }

    /// Runs one CLI in `cwd`. A non-zero exit becomes an error with a short,
    /// path-free detail.
    pub(crate) fn invoke(
        &self,
        app: App,
        args: Vec<OsString>,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Captured, CraftError> {
        let program = self.tools.require(app)?.to_path_buf();
        let inv = Invocation {
            app,
            program: program.clone(),
            args,
            cwd: cwd.to_path_buf(),
            timeout,
        };
        let out = self.runner.run(&inv)?;
        if out.success {
            Ok(out)
        } else {
            Err(CraftError::Failed(
                app,
                failure_detail(&out.stderr, &[cwd, program.as_path()]),
            ))
        }
    }
}

/// Pages and sheets start at 1.
pub(crate) fn check_page(page: u32) -> Result<(), CraftError> {
    if page == 0 {
        Err(CraftError::Invalid("las páginas empiezan en 1"))
    } else {
        Ok(())
    }
}
