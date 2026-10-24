//! The `host` command line, in the library so it can be tested.
//!
//! ```text
//! host [daemon flags]                 run the daemon
//! host pair-code [--ttl S] [--lan IP:PORT]...  mint a single-use pairing code and its link
//! host tls prepare|activate           prepare or activate the LAN certificate
//! host devices list                   list paired devices
//! host devices revoke <id>            revoke one device
//! host devices grant <id> -p <perm>   grant one permission (host-side act)
//! host devices import-gateway <file> [--force]  bring a gateway's devices over
//! ```
//!
//! Every subcommand accepts `--data-dir DIR`; the daemon also accepts
//! `--port`, `--bind`, `--tls-bind`, `--rows`, `--cols`, `--label`, `--hook-ttl`,
//! `--snapshot-lines`, `--workspaces-root` (where projects live, by default
//! `<data-dir>/workspaces`) and repeatable `--spawn "<command line>"`.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use crate::paths;

#[derive(Debug)]
pub struct RunOptions {
    pub port: u16,
    pub bind: IpAddr,
    /// The LAN TLS listener. Off unless named; the address must be concrete.
    pub tls_bind: Option<SocketAddr>,
    pub rows: u16,
    pub cols: u16,
    pub label: String,
    pub spawns: Vec<String>,
    pub data_dir: PathBuf,
    /// Where every project of this host lives. Paths are resolved inside it.
    pub workspaces_root: PathBuf,
    /// Where the OpenCode UAC plugin lives. Without it the seats are not
    /// verified and stay degraded.
    pub uac_plugin_dir: Option<PathBuf>,
    pub hook_ttl: Duration,
    pub snapshot_lines: usize,
}

#[derive(Debug)]
pub enum DevicesCommand {
    List,
    Revoke {
        device_id: String,
    },
    Grant {
        device_id: String,
        permission: String,
    },
    /// Merge the devices of a gateway's `devices.json` into this trust store.
    ImportGateway {
        path: PathBuf,
        /// Replace a registered device whose id arrives with another key.
        force: bool,
    },
}

#[derive(Debug)]
pub enum TlsCommand {
    /// Create the replacement certificate and print its pin.
    Prepare,
    /// Make the prepared certificate the one the LAN listener serves.
    Activate,
}

#[derive(Debug)]
pub enum Command {
    Run(RunOptions),
    PairCode {
        ttl_seconds: f64,
        /// The addresses a device may reach the LAN listener at, for the link.
        lan: Vec<SocketAddr>,
        data_dir: PathBuf,
    },
    Tls {
        command: TlsCommand,
        data_dir: PathBuf,
    },
    HookToken {
        /// `Some(agent id)`: a resident seat token instead of the host token.
        seat: Option<String>,
        data_dir: PathBuf,
    },
    /// The Claude settings the seat writes, for a test harness that runs Claude
    /// by hand: the same file, not a copy. `None` writes to stdout.
    ClaudeSettings {
        hook: PathBuf,
        out: Option<PathBuf>,
    },
    Devices {
        command: DevicesCommand,
        data_dir: PathBuf,
    },
}

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
        hook: hook.ok_or("claude-settings needs --hook <hn-uac-hook binary>")?,
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

fn pair_code(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
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

fn tls(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
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
fn concrete_address(flag: &str, value: String) -> Result<SocketAddr, String> {
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

fn devices(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("devices needs a subcommand: list|revoke|grant|import-gateway")?;
    let mut data_dir = None;
    let mut permission = None;
    let mut force = false;
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
            "--force" => force = true,
            other => positional.push(other.to_string()),
        }
    }
    let command = match sub.as_str() {
        "list" => DevicesCommand::List,
        "import-gateway" => DevicesCommand::ImportGateway {
            path: positional
                .first()
                .map(PathBuf::from)
                .ok_or("import-gateway needs the gateway's devices.json")?,
            force,
        },
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
        other => return Err(format!("unknown devices subcommand: {other}")),
    };
    Ok(Command::Devices {
        command,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

fn run(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
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
    };
    let mut data_dir = None;
    let mut workspaces_root = None;
    let mut uac_plugin_dir = None;
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
    Ok(Command::Run(options))
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

fn resolve_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    match data_dir {
        Some(dir) => Ok(dir),
        None => paths::default_data_dir(),
    }
}

pub const USAGE: &str = "host [--port N] [--bind ADDR] [--rows R] [--cols C] [--label L] \
[--tls-bind IP:PORT] [--hook-ttl SECONDS] [--snapshot-lines N] \
[--workspaces-root DIR] [--uac-plugin-dir DIR] \
[--spawn \"<cmdline>\"]... [--data-dir DIR]\n\
       host pair-code [--ttl SECONDS] [--lan IP:PORT]... [--data-dir DIR]\n\
       host tls prepare|activate [--data-dir DIR]\n\
       host hook-token [--seat AGENT_ID] [--data-dir DIR]\n\
       host claude-settings --hook <hn-uac-hook binary> [--out FILE]\n\
       host devices list [--data-dir DIR]\n\
       host devices revoke <device_id> [--data-dir DIR]\n\
       host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]\n\
       host devices import-gateway <devices.json> [--force] [--data-dir DIR]";

/// The canonical grant command, exactly as the host CLI accepts it. The
/// contracts and the clients' hints quote this text; a test keeps it equal to
/// the CLI usage and to the permission labels.
pub const GRANT_HINT: &str = "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]";

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
    fn the_daemon_defaults_are_loopback_and_a_single_label() {
        let Command::Run(options) = parse(args(&[])).expect("parse") else {
            panic!("expected run");
        };
        assert_eq!(options.bind.to_string(), "127.0.0.1");
        assert_eq!(options.port, 8791);
        assert!(options.spawns.is_empty());
        assert_eq!(options.hook_ttl, crate::DEFAULT_HOOK_TTL);
        assert_eq!(options.snapshot_lines, crate::DEFAULT_SNAPSHOT_LINES);
    }

    #[test]
    fn the_daemon_accepts_the_hook_and_snapshot_knobs() {
        let Command::Run(options) = parse(args(&[
            "--hook-ttl",
            "10",
            "--snapshot-lines",
            "120",
            "--data-dir",
            "C:/tmp/hn",
        ]))
        .expect("parse") else {
            panic!("expected run");
        };
        assert_eq!(options.hook_ttl, Duration::from_secs(10));
        assert_eq!(options.snapshot_lines, 120);
        assert!(parse(args(&["--snapshot-lines", "500"])).is_err());
    }

    #[test]
    fn hook_token_is_a_subcommand() {
        let Command::HookToken { seat, data_dir } =
            parse(args(&["hook-token", "--data-dir", "C:/tmp/hn"])).expect("parse")
        else {
            panic!("expected hook-token");
        };
        assert!(seat.is_none(), "the host token is the default");
        assert!(data_dir.ends_with("hn"));
    }

    #[test]
    fn hook_token_can_name_a_seat() {
        let Command::HookToken { seat, .. } = parse(args(&[
            "hook-token",
            "--seat",
            "ag_1",
            "--data-dir",
            "C:/tmp/hn",
        ]))
        .expect("parse") else {
            panic!("expected hook-token");
        };
        assert_eq!(seat.as_deref(), Some("ag_1"));
    }

    #[test]
    fn claude_settings_needs_the_hook_and_takes_an_optional_out() {
        let Command::ClaudeSettings { hook, out } = parse(args(&[
            "claude-settings",
            "--hook",
            "C:/bin/hn-uac-hook.exe",
            "--out",
            "C:/tmp/settings.json",
        ]))
        .expect("parse") else {
            panic!("expected claude-settings");
        };
        assert!(hook.ends_with("hn-uac-hook.exe"));
        assert!(out.is_some_and(|path| path.ends_with("settings.json")));
        assert!(
            parse(args(&["claude-settings"])).is_err(),
            "--hook is required"
        );
    }

    /// The canonical hint the contracts and the clients quote must be exactly
    /// the usage line the CLI prints, must parse with the real CLI, and must
    /// list exactly the permissions the core knows.
    #[test]
    fn the_grant_hint_matches_the_cli_and_the_permission_labels() {
        assert!(
            USAGE.lines().any(|line| line.trim() == GRANT_HINT),
            "USAGE must contain the canonical grant hint verbatim: {GRANT_HINT}"
        );

        let Command::Devices { command, .. } = parse(args(&[
            "devices",
            "grant",
            "dev-abc",
            "--permission",
            "respond",
            "--data-dir",
            "C:/tmp/hn",
        ]))
        .expect("parse") else {
            panic!("expected devices");
        };
        let DevicesCommand::Grant {
            device_id,
            permission,
        } = command
        else {
            panic!("expected grant");
        };
        assert_eq!(device_id, "dev-abc");
        assert_eq!(permission, "respond");

        let placeholder = GRANT_HINT
            .split("--permission <")
            .nth(1)
            .and_then(|rest| rest.split('>').next())
            .expect("permission placeholder");
        let mut hinted: Vec<&str> = placeholder.split('|').collect();
        hinted.sort_unstable();
        let mut labels: Vec<&str> = hn_core_auth::Permission::ALL
            .iter()
            .map(|permission| permission.label())
            .collect();
        labels.sort_unstable();
        assert_eq!(hinted, labels, "the hint must list every permission id");
    }

    #[test]
    fn the_daemon_accepts_spawns_bind_and_data_dir() {
        let Command::Run(options) = parse(args(&[
            "--port",
            "9000",
            "--bind",
            "100.64.0.1",
            "--spawn",
            "opencode --model x",
            "--spawn",
            "claude",
            "--data-dir",
            "C:/tmp/hn",
        ]))
        .expect("parse") else {
            panic!("expected run");
        };
        assert_eq!(options.port, 9000);
        assert_eq!(options.bind.to_string(), "100.64.0.1");
        assert_eq!(options.spawns.len(), 2);
        assert!(options.data_dir.ends_with("hn"));
    }

    #[test]
    fn wildcard_binds_are_refused() {
        let error = parse(args(&["--bind", "0.0.0.0"])).unwrap_err();
        assert!(error.contains("wildcard"), "{error}");
        assert!(parse(args(&["--bind", "::"])).is_err());
    }

    #[test]
    fn pair_code_and_devices_subcommands_parse() {
        let Command::PairCode { ttl_seconds, .. } =
            parse(args(&["pair-code", "--ttl", "30"])).expect("parse")
        else {
            panic!("expected pair-code");
        };
        assert_eq!(ttl_seconds, 30.0);

        let Command::Devices { command, .. } =
            parse(args(&["devices", "revoke", "dev-abc"])).expect("parse")
        else {
            panic!("expected devices");
        };
        let DevicesCommand::Revoke { device_id } = command else {
            panic!("expected revoke");
        };
        assert_eq!(device_id, "dev-abc");

        assert!(parse(args(&["devices", "grant", "dev-abc"])).is_err());
        let Command::Devices { command, .. } =
            parse(args(&["devices", "grant", "dev-abc", "-p", "respond"])).expect("parse")
        else {
            panic!("expected devices");
        };
        let DevicesCommand::Grant { permission, .. } = command else {
            panic!("expected grant");
        };
        assert_eq!(permission, "respond");

        let Command::Devices { command, .. } = parse(args(&[
            "devices",
            "import-gateway",
            "gw/devices.json",
            "--force",
        ]))
        .expect("parse") else {
            panic!("expected devices");
        };
        let DevicesCommand::ImportGateway { path, force } = command else {
            panic!("expected import-gateway");
        };
        assert_eq!(path, PathBuf::from("gw/devices.json"));
        assert!(force);
        assert!(parse(args(&["devices", "import-gateway"])).is_err());
    }

    #[test]
    fn tls_bind_is_off_by_default_and_needs_a_concrete_address() {
        let Command::Run(options) = parse(args(&[])).expect("parse") else {
            panic!("expected run");
        };
        assert!(options.tls_bind.is_none(), "the LAN listener is opt-in");

        let Command::Run(options) =
            parse(args(&["--tls-bind", "192.168.0.10:8443"])).expect("parse")
        else {
            panic!("expected run");
        };
        assert_eq!(
            options
                .tls_bind
                .map(|address| address.to_string())
                .as_deref(),
            Some("192.168.0.10:8443")
        );

        let error = parse(args(&["--tls-bind", "0.0.0.0:8443"])).unwrap_err();
        assert!(error.contains("wildcard"), "{error}");
        assert!(parse(args(&["--tls-bind", "[::]:8443"])).is_err());
        assert!(
            parse(args(&["--tls-bind", "192.168.0.10"])).is_err(),
            "a port is required"
        );
    }

    #[test]
    fn pair_code_takes_concrete_lan_addresses() {
        let Command::PairCode { lan, .. } = parse(args(&[
            "pair-code",
            "--lan",
            "192.168.0.10:8443",
            "--lan",
            "100.64.0.1:8443",
        ]))
        .expect("parse") else {
            panic!("expected pair-code");
        };
        assert_eq!(lan.len(), 2);
        assert!(parse(args(&["pair-code", "--lan", "0.0.0.0:8443"])).is_err());
    }

    #[test]
    fn tls_is_a_subcommand_with_prepare_and_activate() {
        let Command::Tls { command, data_dir } =
            parse(args(&["tls", "prepare", "--data-dir", "C:/tmp/hn"])).expect("parse")
        else {
            panic!("expected tls");
        };
        assert!(matches!(command, TlsCommand::Prepare));
        assert!(data_dir.ends_with("hn"));

        let Command::Tls { command, .. } =
            parse(args(&["tls", "activate", "--data-dir", "C:/tmp/hn"])).expect("parse")
        else {
            panic!("expected tls");
        };
        assert!(matches!(command, TlsCommand::Activate));

        assert!(parse(args(&["tls"])).is_err());
        assert!(parse(args(&["tls", "rotate", "--data-dir", "C:/tmp/hn"])).is_err());
    }
}
