//! Shared test harness: a real host (pty + ConPTY + detection + API) with a
//! trust store on disk, an agent catalogue and the signed test client.

#![allow(dead_code)]

mod http;
#[allow(unused_imports)]
pub use http::{approval_body, detail, raw_exchange};
mod poll;
#[allow(unused_imports)]
pub use poll::{process_exists, row_of, wait_for_status, wait_until_gone};
mod tool_seat;
use tool_seat::{write_catalog, ToolSeat};

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hn_core_api::{serve, serve_internal, AgentActions, ApiConfig, ApiContext};
use hn_core_auth::{DeviceStore, Permission};
use hn_core_chat::{ChatSeat, ChatStore, HookScope, NewConversation};
use hn_core_host::team::{TeamConfig, TeamService};
use hn_core_host::{paths, AgentHost, HostOptions, SpawnSpec};
use hn_core_pty::PtySize;
use hn_core_seat::{test_profile, SeatConfig, SeatService};
use hn_core_state::StateStore;
use hn_core_test_dir::TestDir;
use hn_core_test_support::{pair_device, FakeSeat, PairResponse, SignedClient};

/// The approval hook binary. It belongs to `hn-core-seat`, which this crate does
/// not depend on, so it is found next to a binary of this crate: the seat
/// refuses to start when it is not there.
fn hook_command() -> PathBuf {
    let name = if cfg!(windows) {
        "hn-uac-hook.exe"
    } else {
        "hn-uac-hook"
    };
    PathBuf::from(env!("CARGO_BIN_EXE_fake-cli")).with_file_name(name)
}

/// What the chat seat does in a test host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeatKind {
    /// Answer every turn (the CORE6A chat flow).
    Fake,
    /// Never answer, so a turn stays queued until a restart.
    Silent,
    /// The real seat, driven by the simulated CLI and the real MCP shim.
    Cli { turn_timeout: Duration },
    /// The real seat with no installed CLI.
    CliMissing,
    /// The team seat: Alfred plus resident project panels (CORE7).
    Team {
        turn_timeout: Duration,
        delegation_timeout: Duration,
        /// Where the UAC plugin lives; the seats are verified against it.
        uac_plugin: Option<PathBuf>,
    },
    /// A seat with no chat turn, only an MCP endpoint.
    Tools,
}

/// The token the tool seat accepts. It is a test constant, never a secret.
pub const TOOL_TOKEN: &str = "test-tool-token";

pub struct TestHost {
    pub addr: SocketAddr,
    pub store: Arc<StateStore>,
    pub devices: Arc<DeviceStore>,
    pub host: Arc<AgentHost>,
    pub chat: Arc<ChatStore>,
    pub seat: Arc<dyn ChatSeat>,
    /// The team service, when the seat is the team: the tests grant seat tokens.
    pub team: Option<Arc<TeamService>>,
    pub tool_url: String,
    pub data_dir: PathBuf,
    pub hook_token: String,
    /// The scratch directory of the host. It is the last field, so it goes
    /// after the stores, whose open database would keep it on Windows; a
    /// restart hands it to the next host.
    scratch: Option<TestDir>,
}

/// The guard of a test host: no agent outlives it. Its directory goes with
/// `scratch`.
impl Drop for TestHost {
    fn drop(&mut self) {
        self.host.kill_all_sessions();
        self.seat.shutdown();
    }
}

impl TestHost {
    pub async fn start(name: &str) -> Self {
        Self::start_with(name, Duration::from_secs(45)).await
    }

    /// A host with a shorter hook TTL, for the freshness tests.
    pub async fn start_with(name: &str, hook_ttl: Duration) -> Self {
        Self::start_with_seat(name, hook_ttl, SeatKind::Fake).await
    }

    pub async fn start_with_seat(name: &str, hook_ttl: Duration, seat: SeatKind) -> Self {
        let scratch = TestDir::new(&format!("core3-tests-{name}"));
        write_catalog(&scratch);
        let data_dir = scratch.to_path_buf();
        Self::open(data_dir, hook_ttl, seat, Some(scratch)).await
    }

    /// A second host over the same data directory: what a restart looks like.
    pub async fn restart(self, hook_ttl: Duration) -> Self {
        Self::restart_with(self, hook_ttl, SeatKind::Fake).await
    }

    pub async fn restart_with(mut self, hook_ttl: Duration, seat: SeatKind) -> Self {
        let scratch = self.scratch.take();
        let data_dir = self.data_dir.clone();
        drop(self);
        Self::open(data_dir, hook_ttl, seat, scratch).await
    }

    async fn open(
        data_dir: PathBuf,
        hook_ttl: Duration,
        seat: SeatKind,
        scratch: Option<TestDir>,
    ) -> Self {
        let devices = Arc::new(DeviceStore::open(Some(paths::devices_path(&data_dir))));
        let store = StateStore::open("test", Some(paths::counters_path(&data_dir)));
        let catalog = Arc::new(hn_core_host::load_agents(&data_dir).expect("catalog"));
        let hook_token = hn_core_host::load_hook_token(&data_dir).expect("hook token");
        let chat =
            Arc::new(ChatStore::open(&paths::chat_db_path(&data_dir)).expect("chat database"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind public");
        let addr = listener.local_addr().expect("public addr");
        let internal = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind internal");
        let internal_addr = internal.local_addr().expect("internal addr");
        let tool_url = format!("http://{internal_addr}/internal/chat/tools");
        let seat_service = match seat {
            SeatKind::Fake | SeatKind::Silent | SeatKind::Tools => None,
            SeatKind::Cli { turn_timeout } => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_hn-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    profiles: vec![test_profile(env!("CARGO_BIN_EXE_fake-cli"))],
                    queue_capacity: 1,
                    turn_timeout,
                })
                .expect("seat service"),
            ),
            SeatKind::CliMissing => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_hn-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    profiles: vec![test_profile("hn-no-such-cli-anywhere")],
                    queue_capacity: 1,
                    turn_timeout: Duration::from_secs(5),
                })
                .expect("seat service"),
            ),
            SeatKind::Team { turn_timeout, .. } => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_hn-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    profiles: vec![test_profile(env!("CARGO_BIN_EXE_fake-cli"))],
                    queue_capacity: 8,
                    turn_timeout,
                })
                .expect("seat service"),
            ),
        };

        let mut options = HostOptions::new(&data_dir, "test");
        options.size = PtySize {
            rows: 30,
            cols: 100,
        };
        options.hook_ttl = hook_ttl;
        options.hook_token = hook_token.clone();
        // The agents' own hooks need the address of the host: the UAC hook
        // opens its approvals there.
        options.hook_url = format!("http://{addr}");
        let host = AgentHost::new(Arc::clone(&store), options).await;

        let mut team_service: Option<Arc<TeamService>> = None;
        let seat: Arc<dyn ChatSeat> = match seat {
            SeatKind::Fake => Arc::new(FakeSeat::replying(Arc::clone(&chat))),
            SeatKind::Silent => Arc::new(FakeSeat::silent()),
            SeatKind::Tools => Arc::new(ToolSeat),
            SeatKind::Cli { .. } | SeatKind::CliMissing => {
                let service = seat_service.expect("the CLI seat needs a service");
                Arc::clone(&service) as Arc<dyn ChatSeat>
            }
            SeatKind::Team {
                delegation_timeout,
                uac_plugin,
                ..
            } => {
                let service = seat_service.expect("the team seat needs a service");
                let team = TeamService::start(TeamConfig {
                    chat: Arc::clone(&chat),
                    host: Arc::clone(&host),
                    alfred: Arc::clone(&service),
                    data_dir: data_dir.clone(),
                    workspaces_root: data_dir.join("workspaces"),
                    delegation_timeout,
                    uac_plugin_dir: uac_plugin,
                })
                .expect("team service");
                team_service = Some(Arc::clone(&team));
                Arc::clone(&team) as Arc<dyn ChatSeat>
            }
        };

        let actions: Arc<dyn AgentActions> = Arc::clone(&host) as Arc<dyn AgentActions>;
        let context = ApiContext::new(ApiConfig {
            store: Arc::clone(&store),
            actions,
            devices: Arc::clone(&devices),
            agents: catalog,
            chat: Arc::clone(&chat),
            seat: Arc::clone(&seat),
            hook_token: Arc::new(hook_token.clone()),
        });
        let internal_context = context.clone();
        tokio::spawn(async move {
            let _ = serve_internal(internal, internal_context).await;
        });
        tokio::spawn(async move {
            let _ = serve(listener, context).await;
        });

        Self {
            addr,
            store,
            devices,
            host,
            chat,
            seat,
            team: team_service,
            tool_url,
            data_dir,
            hook_token,
            scratch,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Mint a single-use pairing code, exactly like `host pair-code` does.
    pub fn pair_code(&self) -> String {
        self.devices
            .issue_enrollment_code(600.0)
            .expect("issue code")
    }

    pub fn grant(&self, device_id: &str, permission: Permission) {
        assert!(
            self.devices.grant(device_id, permission).expect("grant"),
            "device exists"
        );
    }

    /// Pair a fresh device through the real `POST /v1/pair` and return the
    /// signed client bound to it.
    pub async fn pair(&self, name: &str) -> (SignedClient, PairResponse) {
        let mut identity = hn_core_test_support::DemoIdentity::generate("", name);
        let code = self.pair_code();
        let response = pair_device(
            &self.base_url(),
            "p256",
            &identity.public_key_b64(),
            name,
            &code,
        )
        .await
        .expect("pair");
        // The host assigns the device id: adopt it, exactly as the app does.
        identity.set_device_id(&response.device_id);
        (SignedClient::new(&self.base_url(), identity), response)
    }

    /// A paired device with the given grants. A new device is pending and holds
    /// nothing (CORE11, D3), so the suite grants `read_state` first: every
    /// device it pairs can read, and `permissions` adds to that. Tests of the
    /// pending state itself use `pair` and assert it there.
    pub async fn paired_with(&self, name: &str, permissions: &[Permission]) -> SignedClient {
        let (client, paired) = self.pair(name).await;
        self.grant(&paired.device_id, Permission::ReadState);
        for permission in permissions {
            self.grant(&paired.device_id, *permission);
        }
        client
    }

    /// Spawn the fake agent directly (default `approval` scenario).
    pub async fn spawn_fake(&self, extra_args: &[&str]) -> String {
        let mut command = env!("CARGO_BIN_EXE_fake-agent").to_string();
        for arg in extra_args {
            command.push(' ');
            command.push_str(arg);
        }
        let spec = SpawnSpec::parse(&command).expect("spec");
        self.host.start(spec).await.expect("spawn")
    }

    /// POST /v1/hooks/report, as an agent's own hook would.
    pub async fn report_hook(&self, pane_id: &str, state: &str, token: &str) -> (u16, String) {
        let body = serde_json::json!({"pane_id": pane_id, "state": state}).to_string();
        let request = format!(
            "POST /v1/hooks/report HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
             X-HN-Hook-Token: {token}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        raw_exchange(self.addr, request.as_bytes()).await
    }

    /// A live run with its own hook token: exactly what the seat hands to the
    /// CLI of one turn.
    pub fn run_hook(&self, name: &str) -> RunHook {
        let conversation = self
            .chat
            .create_conversation(NewConversation {
                title: Some(name.to_string()),
                project_id: None,
            })
            .expect("conversation");
        let accepted = self
            .chat
            .append_user_message(&conversation.id, &format!("harness-{name}"), "hola")
            .expect("turn");
        let scope = HookScope::run(&accepted.turn.id, &conversation.id, &accepted.turn.agent_id);
        let token = self.chat.issue_hook_token(&scope).expect("hook token");
        RunHook {
            token,
            run_id: accepted.turn.id,
            conversation_id: conversation.id,
        }
    }

    /// A request of the agent's own hook: the per-run token, never a device
    /// signature and never the host token.
    pub async fn hook_request(
        &self,
        token: &str,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> (u16, String) {
        let body = body.unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
             X-HN-Hook-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let mut raw = request.into_bytes();
        raw.extend_from_slice(&body);
        raw_exchange(self.addr, &raw).await
    }
}

/// One run and the credential its hook presents.
pub struct RunHook {
    pub token: String,
    pub run_id: String,
    pub conversation_id: String,
}
