//! `host` — the daemon and its host-side CLI.
//!
//! Run `host` (no subcommand) to supervise agents and serve the local API on
//! loopback. `host pair-code` mints a single-use pairing code (and its link,
//! with `--lan`), `host tls prepare|activate` rotates the LAN certificate,
//! `host hook-token` prints the token agent hooks use, and
//! `host devices list|revoke|grant|recovery` manage the trust store — the
//! same files the daemon re-reads, so both processes agree without a restart.

mod admin;
mod daemon;

use anyhow::Result;
use hn_core_host::cli::{self, Command};

use admin::{claude_settings, devices, hook_token, mail, pair_code, tls};
use daemon::run;

#[tokio::main]
async fn main() -> Result<()> {
    let command = cli::parse(std::env::args().skip(1))
        .map_err(|error| anyhow::anyhow!("{error}\n\n{}", cli::USAGE))?;
    match command {
        Command::PairCode {
            ttl_seconds,
            lan,
            data_dir,
        } => pair_code(ttl_seconds, &lan, &data_dir),
        Command::Tls { command, data_dir } => tls(command, &data_dir),
        Command::HookToken { seat, data_dir } => hook_token(seat.as_deref(), &data_dir),
        Command::ClaudeSettings { hook, out } => claude_settings(&hook, out.as_deref()),
        Command::Devices { command, data_dir } => devices(command, &data_dir),
        Command::Mail(invocation) => mail(invocation).await,
        Command::Run(options) => run(options).await,
    }
}
