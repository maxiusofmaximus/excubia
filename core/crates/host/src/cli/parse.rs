//! The entry point that picks a subcommand from the argument list.

use std::path::PathBuf;

use super::devices::devices;
use super::pairing::{pair_code, tls};
use super::run::run;
use super::types::Command;
use crate::paths;

pub fn parse(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut args = args.peekable();
    // Only consume the first argument when it really is a subcommand.
    match args.peek().map(String::as_str) {
        Some("pair-code") => {
            args.next();
            pair_code(args)
        }
        Some("hook-token") => {
            args.next();
            hook_token(args)
        }
        Some("claude-settings") => {
            args.next();
            claude_settings(args)
        }
        Some("devices") => {
            args.next();
            devices(args)
        }
        Some("tls") => {
            args.next();
            tls(args)
        }
        Some("mail") => {
            args.next();
            crate::mail_cli::parse(args).map(Command::Mail)
        }
        Some("strata") => {
            args.next();
            super::strata::strata(args)
        }
        Some("alfred") => {
            args.next();
            super::alfred::alfred(args)
        }
        Some("audit") => {
            args.next();
            super::audit::audit(args)
        }
        _ => run(args),
    }
}

fn claude_settings(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut hook = None;
    let mut out = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--hook" => hook = Some(PathBuf::from(args.next().ok_or("--hook needs a path")?)),
            "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?)),
            other => return Err(format!("unknown argument for claude-settings: {other}")),
        }
    }
    Ok(Command::ClaudeSettings {
        hook: hook.ok_or("claude-settings needs --hook <excubia-uac-hook binary>")?,
        out,
    })
}

fn hook_token(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut data_dir = None;
    let mut seat = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--seat" => seat = Some(args.next().ok_or("--seat needs an agent id")?),
            other => return Err(format!("unknown argument for hook-token: {other}")),
        }
    }
    Ok(Command::HookToken {
        seat,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

pub(super) fn resolve_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    match data_dir {
        Some(dir) => Ok(dir),
        None => paths::default_data_dir(),
    }
}
