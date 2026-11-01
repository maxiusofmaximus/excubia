//! Pairing codes and the LAN TLS subcommand.

use std::net::SocketAddr;
use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, TlsCommand};

pub(super) fn pair_code(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut ttl_seconds = hn_core_auth::ENROLLMENT_TTL_SECONDS;
    let mut data_dir = None;
    let mut lan = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ttl" => {
                ttl_seconds = args
                    .next()
                    .ok_or("--ttl needs a value")?
                    .parse()
                    .map_err(|_| "--ttl needs a number of seconds".to_string())?;
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--lan" => lan.push(concrete_address(
                "--lan",
                args.next().ok_or("--lan needs an address")?,
            )?),
            other => return Err(format!("unknown argument for pair-code: {other}")),
        }
    }
    Ok(Command::PairCode {
        ttl_seconds,
        lan,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

pub(super) fn tls(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("tls needs a subcommand: prepare|activate")?;
    let mut data_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            other => return Err(format!("unknown argument for tls: {other}")),
        }
    }
    let command = match sub.as_str() {
        "prepare" => TlsCommand::Prepare,
        "activate" => TlsCommand::Activate,
        other => return Err(format!("unknown tls subcommand: {other}")),
    };
    Ok(Command::Tls {
        command,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

/// A concrete address for the LAN listener or a pairing link. A wildcard names
/// no host a device could reach, so it is refused, as `--bind` refuses it.
pub(super) fn concrete_address(flag: &str, value: String) -> Result<SocketAddr, String> {
    let address: SocketAddr = value
        .parse()
        .map_err(|_| format!("{flag} needs an IP:PORT address, got {value:?}"))?;
    if address.ip().is_unspecified() {
        return Err(format!(
            "{flag} refuses wildcard addresses (0.0.0.0, ::): name a concrete address"
        ));
    }
    Ok(address)
}
