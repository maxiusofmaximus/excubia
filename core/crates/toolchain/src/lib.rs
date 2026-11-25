//! Git and GitHub CLI for one Excubia profile: a pinned, verified install under
//! `<data>/toolchain`, the PATH and profile variables its processes inherit, and
//! the GitHub account link that uses them.
//!
//! Nothing here is global, reads a command or path from a model, or touches the
//! user's own Git configuration. See `GITHUB-RESULT.md` and `HERRAMIENTAS-RESULT.md`
//! for the limits.

pub mod archive;
pub mod fetch;
pub mod github;
pub mod install;
pub mod layout;
pub mod manifest;

pub use fetch::{Fetch, HttpsFetch};
pub use install::{ExecProbe, InstallError, Installer, Outcome, Probe};
pub use layout::{Layout, Status};
pub use manifest::{pinned, Distribution, Tool};
