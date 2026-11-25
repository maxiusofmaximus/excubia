//! The `excubia-host toolchain` subcommands: status and install.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, ToolchainCommand};

pub(super) fn toolchain(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args.next().ok_or("toolchain needs a subcommand: status|install")?;
    let mut data_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            other => return Err(format!("unknown argument for toolchain: {other}")),
        }
    }
    let command = match sub.as_str() {
        "status" => ToolchainCommand::Status,
        "install" => ToolchainCommand::Install,
        other => return Err(format!("unknown toolchain subcommand: {other}")),
    };
    Ok(Command::Toolchain {
        command,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> impl Iterator<Item = String> {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn install_and_status_parse_and_nothing_else_does() {
        let Command::Toolchain { command, .. } =
            toolchain(args(&["install", "--data-dir", "C:/tmp/hn"])).expect("parse")
        else {
            panic!("expected toolchain");
        };
        assert_eq!(command, ToolchainCommand::Install);
        assert!(toolchain(args(&["install", "--url", "x"])).is_err());
        assert!(toolchain(args(&["download"])).is_err());
    }
}
