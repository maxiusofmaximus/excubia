//! The `host` command line, in the library so it can be tested.
//!
//! ```text
//! host [daemon flags]                 run the daemon
//! host pair-code [--ttl S] [--lan IP:PORT]...  mint a single-use pairing code and its link
//! host pair-code --pake [--port N]    ask the running daemon for its one-attempt PAKE code
//! host tls prepare|activate           prepare or activate the LAN certificate
//! host devices list                   list paired devices
//! host devices revoke <id>            revoke one device
//! host devices grant <id> -p <perm>   grant one permission (host-side act)
//! host devices recovery <id>          rotate and print a recovery code (once)
//! host audit tail [--n 50]            the last events of the audit log
//! ```
//!
//! Every subcommand accepts `--data-dir DIR`; the daemon also accepts
//! `--port`, `--bind`, `--tls-bind`, `--rows`, `--cols`, `--label`, `--hook-ttl`,
//! `--snapshot-lines`, `--workspaces-root` (where projects live, by default
//! `<data-dir>/workspaces`) and repeatable `--spawn "<command line>"`.
mod alfred;
mod audit;
mod devices;
mod pairing;
mod parse;
mod run;
mod strata;
mod types;
mod usage;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_pairing;

pub use parse::parse;
pub use types::{
    AlfredCommand, Command, DevicesCommand, RunOptions, StrataCommand, TlsCommand, DEFAULT_PORT,
};
pub use usage::{GRANT_HINT, USAGE};
