//! Drives the ArtCraft document CLIs (GridCraft, WordCraft, PdfCraft) for the
//! chat: finds them, runs them with limits, and turns an attachment into text
//! or a PNG preview.
//!
//! Every operation works on a copy of the attachment in a private folder that
//! is removed when the operation ends. The CLIs are never run through a shell.
//! This crate does not talk to Alfred or to the API; the host wires it in.

mod app;
mod create;
mod discover;
mod driver;
mod error;
mod kind;
pub mod ooxml;
mod process;
mod read;
mod render;
mod runner;
mod workspace;

pub use app::App;
pub use discover::{Tools, CONFIG_FILE};
pub use driver::{Document, Driver, VersionReport};
pub use error::CraftError;
pub use kind::Kind;
pub use process::ProcessRunner;
pub use read::{ReadOpts, ReadOut, DEFAULT_RANGE};
pub use render::Preview;
pub use runner::{Captured, Invocation, Runner, MAX_OUTPUT_BYTES};
pub use workspace::{safe_name, Workspace, MAX_INPUT_BYTES};
