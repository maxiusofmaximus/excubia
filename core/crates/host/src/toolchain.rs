//! `excubia-host toolchain status|install`: the pinned Git and GitHub CLI of this profile.
//! Install takes no distribution from the caller: the archives and their digests are pinned
//! in `excubia-toolchain`. The card and the GitHub link use the same installer.

use std::path::Path;

use anyhow::{bail, Result};
use excubia_toolchain::install::{ExecProbe, Installer, Outcome};
use excubia_toolchain::{pinned, HttpsFetch, Layout, Status};

use crate::cli::ToolchainCommand;

pub fn run(command: ToolchainCommand, data_dir: &Path) -> Result<()> {
    let layout = Layout::new(data_dir);
    match command {
        ToolchainCommand::Status => {
            for dist in pinned() {
                match layout.status(&dist) {
                    Status::Missing => println!("{}: falta", dist.label()),
                    Status::Ready { version } => println!(
                        "{}: {version} verificado en {}",
                        dist.label(),
                        layout.exe(&dist).display()
                    ),
                    Status::Broken { reason } => {
                        println!("{}: no válido ({reason})", dist.label())
                    }
                }
            }
            Ok(())
        }
        ToolchainCommand::Install => {
            let installer = Installer {
                layout: &layout,
                fetch: &HttpsFetch,
                probe: &ExecProbe,
            };
            for dist in pinned() {
                match installer.install(&dist) {
                    Ok(Outcome::Installed { version }) => {
                        println!("{} {version}: instalado", dist.label())
                    }
                    Ok(Outcome::AlreadyInstalled { version }) => {
                        println!("{} {version}: ya estaba instalado", dist.label())
                    }
                    Err(error) => {
                        bail!("{} no se instaló ({}): {error}", dist.label(), error.code())
                    }
                }
            }
            Ok(())
        }
    }
}
