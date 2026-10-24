//! APK6 acceptance host: the shipped host service with the test CLI profile.
//!
//! CORE7's `host` only selects shipped CLI profiles, and none of them has a
//! resident panel mode yet. This small wrapper reuses the same crates, data
//! directory and HTTP/MCP surfaces but selects `fake-cli` so the team flow can
//! be exercised end to end without model quota. It is test tooling, not part
//! of the app and not a replacement for the shipped daemon.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use hn_core_api::{serve, serve_internal, AgentActions, ApiConfig, ApiContext};
use hn_core_auth::DeviceStore;
use hn_core_chat::{ChatSeat, ChatStore};
use hn_core_host::team::{TeamConfig, TeamService, DELEGATION_TIMEOUT};
use hn_core_host::{load_agents, load_hook_token, paths, AgentHost, HostOptions};
use hn_core_pty::PtySize;
use hn_core_seat::{test_profile, SeatConfig, SeatService};
use hn_core_state::StateStore;

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = PathBuf::from(required("--data-dir")?);
    let bind: IpAddr = required("--bind")?.parse().context("--bind")?;
    let port: u16 = required("--port")?.parse().context("--port")?;
    let workspaces_root = PathBuf::from(required("--workspaces-root")?);
    let fake_cli = PathBuf::from(required("--fake-cli")?);
    let mcp_command = PathBuf::from(required("--mcp")?);
    if !fake_cli.is_file() {
        bail!("{} is missing", fake_cli.display());
    }
    if !mcp_command.is_file() {
        bail!("{} is missing", mcp_command.display());
    }

    paths::ensure_data_dir(&data_dir)?;
    std::fs::create_dir_all(&workspaces_root).context("workspaces root")?;
    let devices = Arc::new(DeviceStore::open(Some(paths::devices_path(&data_dir))));
    let store = StateStore::open(
        env!("CARGO_PKG_VERSION"),
        Some(paths::counters_path(&data_dir)),
    );
    let catalog = Arc::new(load_agents(&data_dir)?);
    let token = Arc::new(load_hook_token(&data_dir)?);
    let chat = Arc::new(ChatStore::open(&paths::chat_db_path(&data_dir))?);

    let local = SocketAddr::new(bind, port);
    let listener = tokio::net::TcpListener::bind(local)
        .await
        .with_context(|| format!("bind {local}"))?;
    let local = listener.local_addr()?;

    let internal = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("bind internal MCP listener")?;
    let internal_address = internal.local_addr()?;

    let seat_service = SeatService::start(SeatConfig {
        data_dir: data_dir.clone(),
        chat: Arc::clone(&chat),
        mcp_command,
        tool_url: format!("http://{internal_address}/internal/chat/tools"),
        hook_url: format!("http://{local}"),
        profiles: vec![test_profile(fake_cli.to_string_lossy())],
        queue_capacity: 8,
        turn_timeout: Duration::from_secs(600),
    })
    .context("start the test seat")?;
    let selected = seat_service
        .selected()
        .context("fake-cli was not selected as the seat")?;
    println!(
        "test-host: Alfred seat = {} {} ({:?})",
        selected.profile.id.as_str(),
        selected.version,
        selected.profile.mode
    );

    let mut host_options = HostOptions::new(&data_dir, "apk6-test-host");
    host_options.size = PtySize {
        rows: 48,
        cols: 140,
    };
    host_options.hook_token = token.to_string();
    host_options.hook_url = format!("http://{local}");
    let host = AgentHost::new(Arc::clone(&store), host_options).await;

    let team = TeamService::start(TeamConfig {
        chat: Arc::clone(&chat),
        host: Arc::clone(&host),
        alfred: Arc::clone(&seat_service),
        data_dir: data_dir.clone(),
        workspaces_root,
        delegation_timeout: DELEGATION_TIMEOUT,
        uac_plugin_dir: None,
    })
    .context("start the team service")?;
    let seat: Arc<dyn ChatSeat> = Arc::clone(&team) as Arc<dyn ChatSeat>;

    let actions: Arc<dyn AgentActions> = Arc::clone(&host) as Arc<dyn AgentActions>;
    let agent_names = catalog.names().join(", ");
    let context = ApiContext::new(ApiConfig {
        store,
        actions,
        devices,
        agents: catalog,
        chat,
        seat,
        hook_token: token,
    });
    println!("test-host: listening on http://{local} ({agent_names})");
    let mut internal_task = tokio::spawn(serve_internal(internal, context.clone()));
    let outcome: Result<()> = tokio::select! {
        served = serve(listener, context) => served.context("serve the public API"),
        served = &mut internal_task => match served {
            Ok(served) => served.context("serve the internal MCP API"),
            Err(error) => Err(anyhow::anyhow!("internal MCP task: {error}")),
        },
        _ = tokio::signal::ctrl_c() => Ok(()),
    };
    internal_task.abort();
    team.shutdown();
    seat_service.shutdown();
    host.shutdown().await;
    outcome
}

fn required(flag: &str) -> Result<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    args.iter()
        .position(|argument| argument == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
        .with_context(|| format!("{flag} is required"))
}
