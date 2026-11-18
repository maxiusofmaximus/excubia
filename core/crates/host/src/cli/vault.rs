//! `excubia-host vault status|init|recover|finish`: the vault of `chat.db`.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, VaultCommand, DEFAULT_PORT};

pub(super) fn vault(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("vault needs a subcommand: status|init|recover|finish")?;
    let mut data_dir: Option<PathBuf> = None;
    let mut no_mouse = false;
    let mut host_stopped = false;
    let mut port = DEFAULT_PORT;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--no-mouse" => no_mouse = true,
            "--host-stopped" => host_stopped = true,
            "--port" => {
                port = args
                    .next()
                    .ok_or("--port needs a value")?
                    .parse()
                    .map_err(|_| "--port needs a number".to_string())?;
            }
            other => return Err(format!("unknown argument for vault: {other}")),
        }
    }
    let command = match sub.as_str() {
        "status" => VaultCommand::Status,
        "init" => VaultCommand::Init {
            no_mouse,
            host_stopped,
            port,
        },
        "recover" => VaultCommand::Recover,
        "finish" => VaultCommand::Finish,
        other => return Err(format!("unknown vault subcommand: {other}")),
    };
    Ok(Command::Vault {
        command,
        data_dir: resolve_data_dir(data_dir)?,
    })
}
