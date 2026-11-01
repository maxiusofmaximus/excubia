//! `host` without a subcommand: supervise the agents and serve the local API on
//! loopback, and the LAN listener when it is named.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::serve::Listener;
use hn_core_api::voice::{EngineStarter, VoiceService};
use hn_core_api::{
    serve, serve_client, serve_internal, AgentActions, ApiConfig, ApiContext, TlsListener,
};
use hn_core_auth::DeviceStore;
use hn_core_chat::{ChatSeat, ChatStore};
use hn_core_host::calendar_reminders;
use hn_core_host::cli::RunOptions;
use hn_core_host::mail_routes;
use hn_core_host::mail_shadow::Shadow;
use hn_core_host::mail_tools::MailTools;
use hn_core_host::team::{TeamConfig, TeamService, DELEGATION_TIMEOUT};
use hn_core_host::tls_slots::TlsSlots;
use hn_core_host::{
    load_agents, load_hook_token, paths, spawn_alerts, AgentHost, HostOptions, SpawnSpec,
};
use hn_core_pin::Identity;
use hn_core_pty::PtySize;
use hn_core_seat::{shipped, SeatConfig, SeatService};
use hn_core_state::StateStore;
use tokio::task::JoinHandle;

const SEAT_QUEUE: usize = 8;
const SEAT_TIMEOUT: Duration = Duration::from_secs(600);

pub(super) async fn run(options: RunOptions) -> Result<()> {
    paths::ensure_data_dir(&options.data_dir)?;
    let devices = Arc::new(DeviceStore::open(Some(paths::devices_path(
        &options.data_dir,
    ))));
    let store = StateStore::open(
        env!("CARGO_PKG_VERSION"),
        Some(paths::counters_path(&options.data_dir)),
    );
    let chat = ChatStore::open(&paths::chat_db_path(&options.data_dir))
        .context("open the chat database")?;
    chat.set_attachments_root(&paths::attachments_dir(&options.data_dir));
    let chat = Arc::new(chat);
    let alerts_task = spawn_alerts(Arc::clone(&store), Arc::clone(&chat));
    let catalog = Arc::new(load_agents(&options.data_dir)?);
    let token = Arc::new(load_hook_token(&options.data_dir)?);
    calendar_reminders::watch(Arc::clone(&chat), Arc::clone(&store));
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
            bind_lan(address, &identity).await
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

    // The mail surface first: the sends own the ledger of drafts, and the mail
    // tools Alfred may call settle a send's question when its draft changes.
    let mail = mail_routes::open(&options.data_dir, Arc::clone(&chat))
        .map_err(|error| anyhow::anyhow!("open the mail store: {error}"))?;
    let mail_sends = mail
        .sends()
        .map_err(|error| anyhow::anyhow!("open the mail sends: {error}"))?
        .clone();
    let mail_tools = MailTools::open(
        &options.data_dir,
        Arc::clone(&chat),
        &options.workspaces_root,
        mail_sends,
    )
    .map_err(|error| anyhow::anyhow!("open the mail tools: {error}"))?;
    let mail_tools = match &options.mail_sidecar {
        Some(endpoint) => {
            mail_tools.with_shadow(Shadow::for_host(endpoint.clone(), &options.data_dir))
        }
        None => mail_tools,
    };
    let mail_tools = Arc::new(mail_tools);
    mail.attach_tools(Arc::clone(&mail_tools));

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
        mail: mail_tools,
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
    let speech = options
        .asr
        .map(|config| Arc::new(*config) as Arc<dyn EngineStarter>);
    let voice = Arc::new(VoiceService::new(speech));
    Arc::clone(&voice).warm_up();
    let context = ApiContext::new(ApiConfig {
        store,
        actions,
        devices,
        agents: catalog,
        chat,
        seat,
        hook_token: token,
        mail: Arc::new(mail),
        voice,
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
    alerts_task.abort();
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

/// The LAN listener, or `None` when it cannot start: the host then serves only
/// loopback, and a device still reaches it there or over another transport.
async fn bind_lan(address: SocketAddr, identity: &Identity) -> Option<TlsListener> {
    let bound = match hn_core_pin::server_config(identity) {
        Ok(config) => TlsListener::bind(address, Arc::new(config)).await,
        Err(error) => Err(std::io::Error::other(error.to_string())),
    };
    match bound {
        Ok(listener) => {
            println!(
                "host: LAN TLS listening on https://{} (SPKI pinned; device routes only)",
                listener.local_addr().unwrap_or(address)
            );
            Some(listener)
        }
        Err(error) => {
            println!("host: LAN TLS no disponible en {address}: {error}; sigue solo en loopback");
            None
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity::generate().expect("identity")
    }

    #[tokio::test]
    async fn a_busy_lan_port_degrades_to_loopback() {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").expect("busy port");
        let address = busy.local_addr().expect("address");

        assert!(bind_lan(address, &identity()).await.is_none());
    }

    #[tokio::test]
    async fn a_free_lan_port_binds() {
        let address = "127.0.0.1:0".parse().expect("address");

        assert!(bind_lan(address, &identity()).await.is_some());
    }
}
