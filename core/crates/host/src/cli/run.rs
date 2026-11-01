//! The daemon command line: `host [daemon flags]`.

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use hn_core_asr::{AsrConfig, Corrector};

use crate::mail_sidecar::SidecarEndpoint;

use super::pairing::concrete_address;
use super::parse::resolve_data_dir;
use super::types::{Command, RunOptions};
use super::usage::USAGE;

pub(super) fn run(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut options = RunOptions {
        port: 8791,
        bind: IpAddr::from([127, 0, 0, 1]),
        tls_bind: None,
        rows: 30,
        cols: 100,
        label: "core".to_string(),
        spawns: Vec::new(),
        data_dir: PathBuf::new(),
        workspaces_root: PathBuf::new(),
        uac_plugin_dir: None,
        hook_ttl: crate::DEFAULT_HOOK_TTL,
        snapshot_lines: crate::DEFAULT_SNAPSHOT_LINES,
        asr: None,
        mail_sidecar: None,
    };
    let mut data_dir = None;
    let mut workspaces_root = None;
    let mut uac_plugin_dir = None;
    let (mut asr_python, mut asr_script, mut asr_model) = (None, None, None);
    let mut corrector = Corrector::On;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => options.port = parse_arg(&mut args, "--port")?,
            "--rows" => options.rows = parse_arg(&mut args, "--rows")?,
            "--cols" => options.cols = parse_arg(&mut args, "--cols")?,
            "--label" => options.label = args.next().ok_or("--label needs a value")?,
            "--spawn" => options
                .spawns
                .push(args.next().ok_or("--spawn needs a command line")?),
            "--hook-ttl" => {
                let seconds: u64 = parse_arg(&mut args, "--hook-ttl")?;
                options.hook_ttl = Duration::from_secs(seconds);
            }
            "--snapshot-lines" => {
                let lines: usize = parse_arg(&mut args, "--snapshot-lines")?;
                if lines > crate::MAX_SNAPSHOT_LINES {
                    return Err(format!(
                        "--snapshot-lines cannot exceed {}",
                        crate::MAX_SNAPSHOT_LINES
                    ));
                }
                options.snapshot_lines = lines;
            }
            "--bind" => {
                let value = args.next().ok_or("--bind needs an address")?;
                let address: IpAddr = value
                    .parse()
                    .map_err(|_| format!("--bind needs an IP address, got {value:?}"))?;
                if address.is_unspecified() {
                    return Err(
                        "--bind refuses wildcard addresses (0.0.0.0, ::): name the loopback or tailnet address"
                            .to_string(),
                    );
                }
                options.bind = address;
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--tls-bind" => {
                options.tls_bind = Some(concrete_address(
                    "--tls-bind",
                    args.next().ok_or("--tls-bind needs an address")?,
                )?)
            }
            "--workspaces-root" => {
                workspaces_root = Some(PathBuf::from(
                    args.next().ok_or("--workspaces-root needs a value")?,
                ))
            }
            "--uac-plugin-dir" => {
                uac_plugin_dir = Some(PathBuf::from(
                    args.next().ok_or("--uac-plugin-dir needs a value")?,
                ))
            }
            "--asr-python" => asr_python = Some(path_arg(&mut args, "--asr-python")?),
            "--asr-script" => asr_script = Some(path_arg(&mut args, "--asr-script")?),
            "--asr-model" => asr_model = Some(path_arg(&mut args, "--asr-model")?),
            "--asr-no-corrector" => corrector = Corrector::Off,
            "--mail-sidecar" => {
                let url = args.next().ok_or("--mail-sidecar needs a value")?;
                options.mail_sidecar = Some(SidecarEndpoint::parse(&url)?);
            }
            "--help" | "-h" => {
                return Err(USAGE.to_string());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    options.data_dir = resolve_data_dir(data_dir)?;
    options.workspaces_root = match workspaces_root {
        Some(root) => root,
        None => options.data_dir.join("workspaces"),
    };
    options.uac_plugin_dir = uac_plugin_dir;
    options.asr = asr_config(asr_python, asr_script, asr_model)?.map(|mut config| {
        config.corrector = corrector;
        config
    });
    Ok(Command::Run(options))
}

fn path_arg(args: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        args.next().ok_or_else(|| format!("{name} needs a value"))?,
    ))
}

/// The three speech paths go together: a partial set is a mistake, not a host without speech.
fn asr_config(
    python: Option<PathBuf>,
    script: Option<PathBuf>,
    model: Option<PathBuf>,
) -> Result<Option<Box<AsrConfig>>, String> {
    match (python, script, model) {
        (Some(python), Some(script), Some(model)) => {
            Ok(Some(Box::new(AsrConfig::new(python, script, model))))
        }
        (None, None, None) => Ok(None),
        _ => Err("--asr-python, --asr-script and --asr-model go together".to_string()),
    }
}

fn parse_arg<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, String> {
    args.next()
        .ok_or_else(|| format!("{name} needs a value"))?
        .parse()
        .map_err(|_| format!("{name} has an invalid value"))
}
