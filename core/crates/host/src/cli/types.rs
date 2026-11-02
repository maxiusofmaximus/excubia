//! The parsed shapes of the host command line.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use hn_core_asr::AsrConfig;

use crate::mail_sidecar::SidecarEndpoint;

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
    /// The speech engine. Off unless all three `--asr-*` flags are named. Boxed so `Command` stays small.
    pub asr: Option<Box<AsrConfig>>,
    /// The triage sidecar in shadow mode. Off unless `--mail-sidecar` names it.
    pub mail_sidecar: Option<SidecarEndpoint>,
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
    /// Rotate the device's recovery code and print the new one once.
    Recovery {
        device_id: String,
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
    /// `host mail login|accounts|logout`: the mailboxes this host signs in.
    Mail(crate::mail_cli::MailInvocation),
}
