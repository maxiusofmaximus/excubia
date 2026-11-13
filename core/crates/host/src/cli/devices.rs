//! The `excubia-host devices` subcommands: list, revoke, grant and recovery.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, DevicesCommand};
use super::usage::GRANT_HINT;

pub(super) fn devices(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("devices needs a subcommand: list|revoke|grant|recovery")?;
    let mut data_dir = None;
    let mut permission = None;
    let mut positional: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--permission" | "-p" => {
                permission = Some(args.next().ok_or("--permission needs a value")?)
            }
            other => positional.push(other.to_string()),
        }
    }
    let command = match sub.as_str() {
        "list" => DevicesCommand::List,
        "revoke" => DevicesCommand::Revoke {
            device_id: positional
                .first()
                .cloned()
                .ok_or("revoke needs a device id")?,
        },
        "grant" => DevicesCommand::Grant {
            device_id: positional
                .first()
                .cloned()
                .ok_or("grant needs a device id")?,
            permission: permission.ok_or_else(|| format!("missing --permission; {GRANT_HINT}"))?,
        },
        "recovery" => DevicesCommand::Recovery {
            device_id: positional
                .first()
                .cloned()
                .ok_or("recovery needs a device id")?,
        },
        other => return Err(format!("unknown devices subcommand: {other}")),
    };
    Ok(Command::Devices {
        command,
        data_dir: resolve_data_dir(data_dir)?,
    })
}
