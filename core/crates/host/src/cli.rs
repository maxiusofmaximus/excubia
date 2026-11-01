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
mod devices;
mod pairing;
mod parse;
mod run;
mod types;
mod usage;

#[cfg(test)]
mod tests;

pub use parse::parse;
pub use types::{Command, DevicesCommand, RunOptions, TlsCommand};
pub use usage::{GRANT_HINT, USAGE};
