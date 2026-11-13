//! Pairing codes and the LAN TLS subcommand.

use std::net::SocketAddr;
use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, TlsCommand, DEFAULT_PORT};

pub(super) fn pair_code(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut ttl_seconds = excubia_auth::ENROLLMENT_TTL_SECONDS;
    let mut ttl_given = false;
    let mut data_dir = None;
    let mut lan = Vec::new();
    let mut pake = false;
    let mut uri = false;
    let mut port = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ttl" => {
                ttl_given = true;
                ttl_seconds = args
                    .next()
                    .ok_or("--ttl needs a value")?
                    .parse()
                    .map_err(|_| "--ttl needs a number of seconds".to_string())?;
            }
            "--pake" => pake = true,
            "--uri" => uri = true,
            "--port" => {
                port = Some(
                    args.next()
                        .ok_or("--port needs a value")?
                        .parse::<u16>()
                        .map_err(|_| "--port needs a port number".to_string())?,
                )
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
    if pake {
        // The PAKE code lives 180 s in the daemon's memory and its link has no address: these flags would be ignored.
        if ttl_given || !lan.is_empty() {
            return Err("pair-code --pake takes only --port, --uri and --data-dir".to_string());
        }
        return Ok(Command::PairCodePake {
            port: port.unwrap_or(DEFAULT_PORT),
            uri,
            data_dir: resolve_data_dir(data_dir)?,
        });
    }
    if uri {
        return Err(
            "--uri is only for pair-code --pake (the QR link of the PAKE code)".to_string(),
        );
    }
    if port.is_some() {
        return Err("--port is only for pair-code --pake (the daemon's port)".to_string());
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
