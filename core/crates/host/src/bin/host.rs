//! `host` — the daemon and its host-side CLI.
//!
//! Run `host` (no subcommand) to supervise agents and serve the local API on
//! loopback. `host pair-code` mints a single-use pairing code (and its link,
//! with `--lan`), `host tls prepare|activate` rotates the LAN certificate,
//! `host hook-token` prints the token agent hooks use, and
//! `host devices list|revoke|grant` manage the trust store — the same files
//! the daemon re-reads, so both processes agree without a restart.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use axum::serve::Listener;
use hn_core_api::{
    serve, serve_client, serve_internal, AgentActions, ApiConfig, ApiContext, TlsListener,
};
use hn_core_auth::{DeviceStore, Permission};
use hn_core_chat::{ChatSeat, ChatStore, HookScope};
use hn_core_host::cli::{self, Command, DevicesCommand, RunOptions, TlsCommand};
use hn_core_host::team::{TeamConfig, TeamService, DELEGATION_TIMEOUT};
use hn_core_host::tls_slots::TlsSlots;
use hn_core_host::{load_agents, load_hook_token, paths, AgentHost, HostOptions, SpawnSpec};
use hn_core_pin::{unix_now, PairingLink};
use hn_core_pty::PtySize;
use hn_core_seat::{shipped, SeatConfig, SeatService};
use hn_core_state::StateStore;
use tokio::task::JoinHandle;

const SEAT_QUEUE: usize = 8;
const SEAT_TIMEOUT: Duration = Duration::from_secs(600);

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
        Command::Run(options) => run(options).await,
    }
}

/// Mint a single-use pairing code. Only its hash is stored, so the plaintext
/// exists on this screen and nowhere else. With `--lan` it also prints the
/// pairing link: the code, the LAN certificate's pins and the addresses to
/// reach. The pins make a scanned device trust this host's certificate and no
/// other.
fn pair_code(ttl_seconds: f64, lan: &[SocketAddr], data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let store = DeviceStore::open(Some(paths::devices_path(data_dir)));
    let code = store.issue_enrollment_code(ttl_seconds)?;
    println!("pairing code: {code}");
    println!("single use, valid for {ttl_seconds:.0} seconds; enter it on the device");
    if !lan.is_empty() {
        let link = PairingLink {
            code,
            pins: TlsSlots::open(data_dir).pins()?,
            addresses: lan.to_vec(),
            expires_at: unix_now() + ttl_seconds.round() as i64,
        };
        println!("pairing link: {}", link.to_uri());
    }
    println!("data dir: {}", data_dir.display());
    Ok(())
}

/// Prepare or activate the LAN certificate (`tls_slots`). A device can only
/// accept the backup pin if it was given it, and it is given it only by a link
/// printed while that pin is prepared. Rotation therefore reaches a device
/// that paired or re-paired in that window; the daemon must restart to serve
/// the new certificate.
fn tls(command: TlsCommand, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let slots = TlsSlots::open(data_dir);
    match command {
        TlsCommand::Prepare => {
            let pin = slots.prepare()?;
            println!("prepared pin: {pin}");
            println!("links printed from now on carry it as the backup pin");
        }
        TlsCommand::Activate => {
            let (active, previous) = slots.activate()?;
            println!("active pin: {active}");
            println!(
                "previous pin: {previous}; links printed from now on carry it as the backup pin"
            );
            println!("restart the daemon so the LAN listener serves the new certificate");
        }
    }
    println!("data dir: {}", data_dir.display());
    Ok(())
}

/// Print (or write) the Claude settings the seat gives each run: the same bytes
/// `launch` writes, so a harness that runs Claude by hand cannot drift from it.
fn claude_settings(hook: &Path, out: Option<&Path>) -> Result<()> {
    let bytes = hn_core_seat::claude_settings(hook).context("encode the Claude settings")?;
    match out {
        Some(path) => {
            std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?
        }
        None => println!("{}", String::from_utf8_lossy(&bytes)),
    }
    Ok(())
}

/// Print the token agent hooks authenticate with. It lives in the data
/// directory; the daemon hands it to each agent it starts. With `--seat`, it
/// mints a seat token instead: it covers the seat's approvals (what a resident
/// panel carries), and it is stored hashed and revoked by the seat's lifecycle.
fn hook_token(seat: Option<&str>, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let token = match seat {
        None => load_hook_token(data_dir)?,
        Some(agent_id) => {
            let chat = ChatStore::open(&paths::chat_db_path(data_dir))
                .context("open the chat database")?;
            chat.seat(agent_id)
                .with_context(|| format!("no such seat: {agent_id}"))?;
            chat.issue_hook_token(&HookScope::seat(agent_id))?
        }
    };
    println!("hook token: {token}");
    println!("sent as the X-HN-Hook-Token header to POST /v1/hooks/report (loopback only)");
    println!("spawned agents receive it in HN_HOOK_TOKEN");
    println!("data dir: {}", data_dir.display());
    Ok(())
}

fn devices(command: DevicesCommand, data_dir: &Path) -> Result<()> {
    paths::ensure_data_dir(data_dir)?;
    let store = DeviceStore::open(Some(paths::devices_path(data_dir)));
    match command {
        DevicesCommand::List => {
            let devices = store.list();
            if devices.is_empty() {
                println!("no devices paired (data dir: {})", data_dir.display());
            }
            for device in devices {
                let label = device.label.as_deref().unwrap_or("-");
                let revoked = if device.revoked { " REVOKED" } else { "" };
                let permissions: Vec<&str> = device
                    .permissions
                    .iter()
                    .map(|permission| permission.label())
                    .collect();
                println!(
                    "{}  {}  {}  [{}]{}",
                    device.device_id,
                    device.curve.label(),
                    label,
                    permissions.join(","),
                    revoked
                );
            }
            Ok(())
        }
        DevicesCommand::Revoke { device_id } => {
            if store.revoke(&device_id)? {
                println!("revoked {device_id}");
                Ok(())
            } else {
                bail!("no such device: {device_id}")
            }
        }
        DevicesCommand::Grant {
            device_id,
            permission,
        } => {
            let permission = Permission::from_label(&permission)
                .with_context(|| format!("unknown permission: {permission}"))?;
            if store.grant(&device_id, permission)? {
                println!("granted {} to {device_id}", permission.label());
                Ok(())
            } else {
                bail!("no such device: {device_id}")
            }
        }
    }
}

async fn run(options: RunOptions) -> Result<()> {
    paths::ensure_data_dir(&options.data_dir)?;
    let devices = Arc::new(DeviceStore::open(Some(paths::devices_path(
        &options.data_dir,
    ))));
    let store = StateStore::open(
        env!("CARGO_PKG_VERSION"),
        Some(paths::counters_path(&options.data_dir)),
    );
    let catalog = Arc::new(load_agents(&options.data_dir)?);
    let token = Arc::new(load_hook_token(&options.data_dir)?);
    let chat = Arc::new(
        ChatStore::open(&paths::chat_db_path(&options.data_dir))
            .context("open the chat database")?,
    );
    let address = SocketAddr::new(options.bind, options.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("bind {address}"))?;
    let local = listener.local_addr()?;
    if !options.bind.is_loopback() {
        println!(
            "host: warning: bound to {} (non-loopback); every device must be paired and signed, \
             and the address should be a Tailscale address",
            options.bind
        );
    }

    // The LAN listener is off unless named. It serves the device routes only:
    // hooks and the internal surface stay on loopback listeners, so no LAN
    // device can reach them whatever the request says.
    let lan_listener = match options.tls_bind {
        Some(address) => {
            let identity = TlsSlots::open(&options.data_dir).active()?;
            let config = Arc::new(hn_core_pin::server_config(&identity)?);
            let bound = TlsListener::bind(address, config)
                .await
                .with_context(|| format!("bind {address}"))?;
            println!(
                "host: LAN TLS listening on https://{} (SPKI pinned; device routes only)",
                bound.local_addr()?
            );
            Some(bound)
        }
        None => None,
    };

    // The MCP shim is a separate process on a dedicated loopback listener.
    // Keeping it off the public socket means a tailnet peer can never reach it,
    // even before the per-run token is checked.
    let internal = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("bind the internal MCP listener")?;
    let internal_address = internal.local_addr()?;
    let mcp_command = sibling_binary("hn-mcp")?;
    let seat_service = SeatService::start(SeatConfig {
        data_dir: options.data_dir.clone(),
        chat: Arc::clone(&chat),
        mcp_command,
        tool_url: format!("http://{internal_address}/internal/chat/tools"),
        hook_url: format!("http://{local}"),
        hook_command: sibling_binary("hn-uac-hook")?,
        profiles: shipped(),
        queue_capacity: SEAT_QUEUE,
        turn_timeout: SEAT_TIMEOUT,
    })
    .map_err(|error| anyhow::anyhow!("start Alfred's seat: {error}"))?;
    match seat_service.selected() {
        Some(selected) => println!(
            "host: Alfred seat = {} {} ({:?})",
            selected.profile.id.as_str(),
            selected.version,
            selected.profile.mode
        ),
        None => println!(
            "host: warning: no supported CLI found (or hn-mcp is missing); chat answers 409 seat_unavailable"
        ),
    }

    let mut host_options = HostOptions::new(&options.data_dir, options.label.clone());
    host_options.size = PtySize {
        rows: options.rows,
        cols: options.cols,
    };
    host_options.hook_ttl = options.hook_ttl;
    host_options.snapshot_lines = options.snapshot_lines;
    host_options.hook_token = token.to_string();
    host_options.hook_url = format!("http://{local}");
    let host = AgentHost::new(Arc::clone(&store), host_options).await;

    // The team seat is the only surface the API knows: it routes Alfred's
    // chat turns and every project seat's MCP calls.
    let team = TeamService::start(TeamConfig {
        chat: Arc::clone(&chat),
        host: Arc::clone(&host),
        alfred: Arc::clone(&seat_service),
        data_dir: options.data_dir.clone(),
        workspaces_root: options.workspaces_root.clone(),
        delegation_timeout: DELEGATION_TIMEOUT,
        uac_plugin_dir: options.uac_plugin_dir.clone(),
    })
    .map_err(|error| anyhow::anyhow!("start the team seat: {error}"))?;
    println!(
        "host: projects live under {} (canonical)",
        options.workspaces_root.display()
    );
    match &options.uac_plugin_dir {
        Some(dir) => println!("host: UAC plugin verified from {}", dir.display()),
        None => println!(
            "host: warning: no --uac-plugin-dir; the seats stay degraded (no blocking UAC)"
        ),
    }
    let seat: Arc<dyn ChatSeat> = Arc::clone(&team) as Arc<dyn ChatSeat>;

    for command_line in &options.spawns {
        let spec = SpawnSpec::parse(command_line)?;
        let pane_id = host.start(spec).await?;
        println!("host: agent {pane_id} = {command_line}");
    }

    println!(
        "host: listening on http://{local} ({} device(s) paired, agents: {})",
        devices.len(),
        catalog.names().join(", ")
    );
    println!("host: data dir {}", options.data_dir.display());
    println!("host: hooks POST http://{local}/v1/hooks/report (token: host hook-token)");

    let actions: Arc<dyn AgentActions> = Arc::clone(&host) as Arc<dyn AgentActions>;
    let context = ApiContext::new(ApiConfig {
        store,
        actions,
        devices,
        agents: catalog,
        chat,
        seat,
        hook_token: token,
    });
    let mut internal_task = tokio::spawn(serve_internal(internal, context.clone()));
    let mut lan_task =
        lan_listener.map(|listener| tokio::spawn(serve_client(listener, context.clone())));
    let outcome: Result<()> = tokio::select! {
        served = serve(listener, context) => served.context("serve the public API"),
        served = &mut internal_task => match served {
            Ok(served) => served.context("serve the internal MCP API"),
            Err(error) => Err(anyhow::anyhow!("internal MCP task: {error}")),
        },
        served = lan_outcome(&mut lan_task) => served.context("serve the LAN TLS API"),
        _ = tokio::signal::ctrl_c() => {
            println!("host: shutting down");
            Ok(())
        }
    };
    internal_task.abort();
    if let Some(task) = &lan_task {
        task.abort();
    }
    team.shutdown();
    seat_service.shutdown();
    host.shutdown().await;
    outcome
}

/// The LAN listener's outcome, or a future that never resolves when the LAN
/// listener is off, so the select keeps serving the other listeners.
async fn lan_outcome(task: &mut Option<JoinHandle<std::io::Result<()>>>) -> std::io::Result<()> {
    match task {
        Some(task) => task.await.map_err(std::io::Error::other)?,
        None => std::future::pending().await,
    }
}

/// The daemon's sibling binary (`hn-mcp.exe` next to `host.exe`). Cargo builds
/// it only when all binaries are requested, so the host reports a clear
/// `seat_unavailable` if it is missing instead of guessing a path.
fn sibling_binary(name: &str) -> Result<PathBuf> {
    let executable = std::env::current_exe().context("locate the host executable")?;
    let directory = executable
        .parent()
        .context("the host executable has no parent directory")?;
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(directory.join(file))
}
