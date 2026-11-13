//! Shared test harness: a real host (pty + ConPTY + detection + API) with a
//! trust store on disk, an agent catalogue and the signed test client.

#![allow(dead_code)]

pub mod docs;
mod http;
#[allow(unused_imports)]
pub use http::{approval_body, detail, raw_exchange};
mod poll;
#[allow(unused_imports)]
pub use poll::{process_exists, row_of, wait_for_status, wait_until_gone};
mod tool_seat;
use tool_seat::{write_catalog, ToolSeat};
/// The routed fake Microsoft of the send tests, shared with the HTTP tests.
#[path = "../mail_send_support/mod.rs"]
pub mod mail_send_support;
use mail_send_support::{seed_owner, Routed, BODY};

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use excubia_api::voice::{EngineStarter, VoiceService};
use excubia_api::{serve, serve_internal, AgentActions, AlfredBackend, ApiConfig, ApiContext};
use excubia_auth::{DeviceStore, Permission};
use excubia_chat::{ChatSeat, ChatStore, HookScope, NewConversation};
use excubia_crafting::App;
use excubia_host::alfred_switch::{Engine, Switch};
use excubia_host::audit::AuditLog;
use excubia_host::doc_tools::DocTools;
use excubia_host::mail_api::MailLogins;
use excubia_host::mail_cli::ACCOUNTS_DIR;
use excubia_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use excubia_host::mail_tools::{MailTools, LEDGER_FILE};
use excubia_host::pairing_notice::PairingNotice;
use excubia_host::strata::State;
use excubia_host::team::{TeamConfig, TeamService};
use excubia_host::{paths, AgentHost, HostOptions, SpawnSpec, DEFAULT_HOOK_TTL};
use excubia_mail::{AccountStore, FileKey, HttpTransport, Ledger, ReqwestTransport};
use excubia_pty::PtySize;
use excubia_seat::{shipped, test_profile, CliId, CliProfile, SeatConfig, SeatService, TurnEnv};
use excubia_state::StateStore;
use excubia_test_dir::TestDir;
use excubia_test_support::{pair_device, FakeSeat, PairResponse, SignedClient};

/// The approval hook binary. It belongs to `excubia-seat`, which this crate does
/// not depend on, so it is found next to a binary of this crate: the seat
/// refuses to start when it is not there.
/// The test seat's turn environment: the `alfred-local` marker of its data directory.
fn alfred_turn_env(data_dir: &Path) -> Arc<dyn Fn() -> TurnEnv + Send + Sync> {
    let data_dir = data_dir.to_path_buf();
    Arc::new(move || excubia_host::alfred_backend::turn_env(&data_dir))
}

/// The real OpenCode of the test: `EXCUBIA_OPENCODE_BIN` names it, the shipped command otherwise.
pub fn real_opencode() -> CliProfile {
    let mut profile = shipped()
        .into_iter()
        .find(|profile| profile.id == CliId::OpenCode)
        .expect("the shipped OpenCode profile");
    if let Ok(binary) = std::env::var("EXCUBIA_OPENCODE_BIN") {
        if !binary.trim().is_empty() {
            profile.command = binary;
        }
    }
    profile
}

/// The turn environment of the real OpenCode: the local provider answers the turn,
/// and the plugin waits `ttl_seconds` for the operator.
fn opencode_turn_env(
    data_dir: &Path,
    provider_url: &str,
    ttl_seconds: u64,
) -> Arc<dyn Fn() -> TurnEnv + Send + Sync> {
    let data_dir = data_dir.to_path_buf();
    let provider_url = provider_url.to_string();
    Arc::new(move || {
        let mut env = excubia_host::alfred_backend::turn_env(&data_dir);
        env.set.push((
            "OPENCODE_CONFIG_CONTENT".to_string(),
            opencode_provider(&provider_url),
        ));
        env.set.push((
            "EXCUBIA_UAC_TTL_SECONDS".to_string(),
            ttl_seconds.to_string(),
        ));
        env.set
            .push(("EXCUBIA_UAC_POLL_MS".to_string(), "250".to_string()));
        env
    })
}

/// The inline OpenCode configuration that points its model at the local provider.
fn opencode_provider(provider_url: &str) -> String {
    serde_json::json!({
        "model": "mock/mock",
        "provider": {
            "mock": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Mock",
                "options": { "baseURL": provider_url, "apiKey": "dummy" },
                "models": {
                    "mock": { "name": "Mock", "tool_call": true, "limit": { "context": 200000, "output": 8192 } }
                }
            }
        }
    })
    .to_string()
}

/// How many times the seat reported a quota-exhausted turn to its test host.
pub static QUOTA_CALLS: AtomicUsize = AtomicUsize::new(0);

fn quota_counter() -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(|| {
        QUOTA_CALLS.fetch_add(1, Ordering::SeqCst);
    })
}

/// Strata's stand-in for the alfred switch tests: it launches nothing. `start` marks the voice the
/// way Strata does and fails the way Strata does, leaving no marker behind.
#[derive(Default)]
pub struct FakeEngine {
    /// The model answers: `state` reports it ready.
    pub ready: AtomicBool,
    /// No Strata binary: `start` fails at once.
    pub missing: AtomicBool,
    /// `start` waits here until the test clears it: a cold load in progress.
    pub hold: AtomicBool,
    /// How many `start` calls the switch made.
    pub starts: AtomicUsize,
    /// Set once a start is under way (its voice mark is down, the model not yet up).
    pub entered: AtomicBool,
}

impl Engine for FakeEngine {
    fn state(&self) -> anyhow::Result<State> {
        Ok(if self.ready.load(Ordering::SeqCst) {
            State::Ready
        } else {
            State::Stopped
        })
    }

    fn start(&self, data_dir: &Path) -> anyhow::Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        excubia_host::local_mode::set(data_dir)?;
        self.entered.store(true, Ordering::SeqCst);
        let outcome = self.launch();
        if outcome.is_err() {
            excubia_host::alfred_backend::clear(data_dir)?;
            excubia_host::local_mode::clear(data_dir)?;
        }
        outcome
    }

    fn stop(&self, data_dir: &Path) -> anyhow::Result<()> {
        self.ready.store(false, Ordering::SeqCst);
        excubia_host::alfred_backend::clear(data_dir)?;
        excubia_host::local_mode::clear(data_dir)?;
        Ok(())
    }
}

impl FakeEngine {
    fn launch(&self) -> anyhow::Result<()> {
        if self.missing.load(Ordering::SeqCst) {
            anyhow::bail!("no encuentro Strata en la ruta de prueba");
        }
        // Bounded: a test that panics while holding must not hang the runtime's shutdown.
        let until = std::time::Instant::now() + Duration::from_secs(30);
        while self.hold.load(Ordering::SeqCst) {
            anyhow::ensure!(
                std::time::Instant::now() < until,
                "the test never released the start"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.ready.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn hook_command() -> PathBuf {
    let name = if cfg!(windows) {
        "excubia-uac-hook.exe"
    } else {
        "excubia-uac-hook"
    };
    PathBuf::from(env!("CARGO_BIN_EXE_fake-cli")).with_file_name(name)
}

/// The mail logins of a test host, sending included. It has no app registration,
/// so no login reaches Microsoft, and its accounts sit in the scratch data dir
/// under a key file of their own: a test never touches the platform's credential
/// store. Every Graph call goes to `transport`.
pub fn test_mail(
    data_dir: &Path,
    chat: Arc<ChatStore>,
    transport: Arc<dyn HttpTransport>,
) -> MailLogins {
    let accounts = data_dir.join("mail/accounts");
    let key = FileKey::new(data_dir.join("mail-test.key"));
    let store = Arc::new(AccountStore::open(&accounts, &key).expect("mail store"));
    let ledger = Ledger::open(&data_dir.join(LEDGER_FILE)).expect("mail ledger");
    let policy = SendPolicy {
        per_hour: 10,
        window: DEFAULT_WINDOW,
    };
    let sends = MailSends::new(
        Arc::clone(&transport),
        Arc::clone(&store),
        Some("app".to_owned()),
        ledger,
        &accounts,
        Arc::clone(&chat),
        policy,
    );
    MailLogins::with_sends(transport, store, None, chat, sends)
}

/// The transport of a test host's mail: the one a test hands over, or a real
/// client, which no test reaches while the host has no registration.
pub fn mail_transport(given: Option<Arc<dyn HttpTransport>>) -> Arc<dyn HttpTransport> {
    match given {
        Some(transport) => transport,
        None => Arc::new(ReqwestTransport::new().expect("transport")),
    }
}

/// The `mail.*` tools of a test host, over the same store as [`test_mail`] and
/// the sends of that host. Without a registration no Graph call is made.
pub fn test_mail_tools(
    data_dir: &Path,
    chat: Arc<ChatStore>,
    workspaces_root: &Path,
    sends: MailSends,
) -> MailTools {
    let key = FileKey::new(data_dir.join("mail-test.key"));
    let store = AccountStore::open(data_dir.join("mail/accounts"), &key).expect("mail store");
    let transport = ReqwestTransport::new().expect("transport");
    MailTools::new(
        Arc::new(transport),
        Arc::new(store),
        None,
        data_dir,
        chat,
        workspaces_root,
        sends,
    )
    .expect("mail tools")
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
    /// The real OpenCode with the UAC plugin: its turn asks `provider_url` for a
    /// shell command, and the plugin decides whether that command runs.
    OpenCode {
        turn_timeout: Duration,
        /// The plugin the seat loads.
        plugin_dir: PathBuf,
        /// The local provider of the turn (a `CanaryProvider`): no quota is spent.
        provider_url: String,
        /// How long the plugin waits for the operator, in seconds.
        approval_ttl_seconds: u64,
    },
    /// The default seat is the simulated CLI; the `alfred-engine` marker names OpenCode, which
    /// listed as ready. A turn that asks for an approval therefore ran through the marker.
    OpenCodeEngine {
        turn_timeout: Duration,
        plugin_dir: PathBuf,
        provider_url: String,
        approval_ttl_seconds: u64,
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
    /// The host's certificate pin, the input of each pending device's SAS.
    pub host_pin: excubia_pin::Fingerprint,
    /// The host's in-memory PAKE pairing code slot (the context's own).
    pub pake_slot: Arc<excubia_api::PakeSlot>,
    /// The alfred switch of the host, over [`Self::alfred_engine`] (a double).
    pub alfred: Arc<Switch>,
    pub alfred_engine: Arc<FakeEngine>,
    /// The document CLIs of the host: every run is recorded here (DOC5B).
    pub cli: Arc<docs::FakeCli>,
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
        Self::open(
            data_dir,
            hook_ttl,
            seat,
            Some(scratch),
            None,
            None,
            &App::ALL,
        )
        .await
    }

    /// A host whose speech engine is `starter`; `None` is a host without speech.
    pub async fn start_with_voice(name: &str, starter: Option<Arc<dyn EngineStarter>>) -> Self {
        let scratch = TestDir::new(&format!("core3-tests-{name}"));
        write_catalog(&scratch);
        let data_dir = scratch.to_path_buf();
        Self::open(
            data_dir,
            DEFAULT_HOOK_TTL,
            SeatKind::Fake,
            Some(scratch),
            None,
            starter,
            &App::ALL,
        )
        .await
    }

    /// A host whose mail goes to a routed fake Microsoft: `ACCOUNT` is signed in
    /// with sending granted and owns `DRAFT`. The fake comes back, so a test can
    /// read what Microsoft was asked and what it answered.
    pub async fn start_with_mail(name: &str) -> (Self, Arc<Routed>) {
        let scratch = TestDir::new(&format!("core3-tests-{name}"));
        write_catalog(&scratch);
        let data_dir = scratch.to_path_buf();
        let world = Arc::new(Routed::new(BODY));
        let transport: Arc<dyn HttpTransport> = world.clone();
        let store = AccountStore::open(
            data_dir.join("mail/accounts"),
            &FileKey::new(data_dir.join("mail-test.key")),
        )
        .expect("mail store");
        let ledger = Ledger::open(&data_dir.join(LEDGER_FILE)).expect("mail ledger");
        seed_owner(&store, &ledger, transport.as_ref()).await;
        let host = Self::open(
            data_dir,
            DEFAULT_HOOK_TTL,
            SeatKind::Fake,
            Some(scratch),
            Some(transport),
            None,
            &App::ALL,
        )
        .await;
        (host, world)
    }

    /// A host whose document CLIs are `apps` (the others are missing, DOC5B).
    pub async fn start_with_docs(name: &str, apps: &[App]) -> Self {
        let scratch = TestDir::new(&format!("core3-tests-{name}"));
        write_catalog(&scratch);
        let data_dir = scratch.to_path_buf();
        Self::open(
            data_dir,
            DEFAULT_HOOK_TTL,
            SeatKind::Fake,
            Some(scratch),
            None,
            None,
            apps,
        )
        .await
    }

    /// A team host (Alfred and the project seats) whose document CLIs are `apps`.
    pub async fn start_team_with_docs(name: &str, apps: &[App]) -> Self {
        let scratch = TestDir::new(&format!("core3-tests-{name}"));
        write_catalog(&scratch);
        let data_dir = scratch.to_path_buf();
        let seat = SeatKind::Team {
            turn_timeout: Duration::from_secs(30),
            delegation_timeout: Duration::from_secs(30),
            uac_plugin: None,
        };
        Self::open(
            data_dir,
            DEFAULT_HOOK_TTL,
            seat,
            Some(scratch),
            None,
            None,
            apps,
        )
        .await
    }

    /// A second host over the same data directory: what a restart looks like.
    pub async fn restart(self, hook_ttl: Duration) -> Self {
        Self::restart_with(self, hook_ttl, SeatKind::Fake).await
    }

    pub async fn restart_with(mut self, hook_ttl: Duration, seat: SeatKind) -> Self {
        let scratch = self.scratch.take();
        let data_dir = self.data_dir.clone();
        drop(self);
        Self::open(data_dir, hook_ttl, seat, scratch, None, None, &App::ALL).await
    }

    async fn open(
        data_dir: PathBuf,
        hook_ttl: Duration,
        seat: SeatKind,
        scratch: Option<TestDir>,
        mail: Option<Arc<dyn HttpTransport>>,
        voice: Option<Arc<dyn EngineStarter>>,
        apps: &[App],
    ) -> Self {
        let devices = Arc::new(DeviceStore::open(Some(paths::devices_path(&data_dir))));
        let host_pin = excubia_pin::Fingerprint::of_spki(b"harness host certificate");
        let store = StateStore::open("test", Some(paths::counters_path(&data_dir)));
        let catalog = Arc::new(excubia_host::load_agents(&data_dir).expect("catalog"));
        let hook_token = excubia_host::load_hook_token(&data_dir).expect("hook token");
        let chat =
            Arc::new(ChatStore::open(&paths::chat_db_path(&data_dir)).expect("chat database"));
        chat.set_attachments_root(&paths::attachments_dir(&data_dir));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind public");
        let addr = listener.local_addr().expect("public addr");
        let internal = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind internal");
        let internal_addr = internal.local_addr().expect("internal addr");
        let tool_url = format!("http://{internal_addr}/internal/chat/tools");
        let cli = Arc::new(docs::FakeCli::default());
        let craft = Arc::new(docs::craft(&data_dir, apps, Arc::clone(&cli)));
        let seat_service = match seat.clone() {
            SeatKind::Fake | SeatKind::Silent | SeatKind::Tools => None,
            SeatKind::OpenCode {
                turn_timeout,
                plugin_dir,
                provider_url,
                approval_ttl_seconds,
            } => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_excubia-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    plugin_dir: Some(plugin_dir),
                    profiles: vec![real_opencode()],
                    queue_capacity: 1,
                    turn_timeout,
                    turn_env: opencode_turn_env(&data_dir, &provider_url, approval_ttl_seconds),
                    engines: Vec::new(),
                    turn_engine: Arc::new(|| None),
                    on_quota_exhausted: quota_counter(),
                })
                .expect("seat service"),
            ),
            SeatKind::Cli { turn_timeout } => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_excubia-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    plugin_dir: None,
                    profiles: vec![test_profile(env!("CARGO_BIN_EXE_fake-cli"))],
                    queue_capacity: 1,
                    turn_timeout,
                    turn_env: alfred_turn_env(&data_dir),
                    engines: Vec::new(),
                    turn_engine: Arc::new(|| None),
                    on_quota_exhausted: quota_counter(),
                })
                .expect("seat service"),
            ),
            SeatKind::OpenCodeEngine {
                turn_timeout,
                plugin_dir,
                provider_url,
                approval_ttl_seconds,
            } => {
                let engines = excubia_host::ready_engines(&[real_opencode()], &[CliId::OpenCode]);
                let marker_dir = data_dir.clone();
                Some(
                    SeatService::start(SeatConfig {
                        data_dir: data_dir.clone(),
                        chat: Arc::clone(&chat),
                        mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_excubia-mcp")),
                        tool_url: tool_url.clone(),
                        hook_url: format!("http://{addr}"),
                        hook_command: hook_command(),
                        plugin_dir: Some(plugin_dir),
                        profiles: vec![test_profile(env!("CARGO_BIN_EXE_fake-cli"))],
                        queue_capacity: 1,
                        turn_timeout,
                        turn_env: opencode_turn_env(&data_dir, &provider_url, approval_ttl_seconds),
                        engines,
                        turn_engine: Arc::new(move || {
                            excubia_host::alfred_backend::engine(&marker_dir)
                        }),
                        on_quota_exhausted: quota_counter(),
                    })
                    .expect("seat service"),
                )
            }
            SeatKind::CliMissing => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_excubia-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    plugin_dir: None,
                    profiles: vec![test_profile("excubia-no-such-cli-anywhere")],
                    queue_capacity: 1,
                    turn_timeout: Duration::from_secs(5),
                    turn_env: alfred_turn_env(&data_dir),
                    engines: Vec::new(),
                    turn_engine: Arc::new(|| None),
                    on_quota_exhausted: quota_counter(),
                })
                .expect("seat service"),
            ),
            SeatKind::Team { turn_timeout, .. } => Some(
                SeatService::start(SeatConfig {
                    data_dir: data_dir.clone(),
                    chat: Arc::clone(&chat),
                    mcp_command: PathBuf::from(env!("CARGO_BIN_EXE_excubia-mcp")),
                    tool_url: tool_url.clone(),
                    hook_url: format!("http://{addr}"),
                    hook_command: hook_command(),
                    plugin_dir: None,
                    profiles: vec![test_profile(env!("CARGO_BIN_EXE_fake-cli"))],
                    queue_capacity: 8,
                    turn_timeout,
                    turn_env: alfred_turn_env(&data_dir),
                    engines: Vec::new(),
                    turn_engine: Arc::new(|| None),
                    on_quota_exhausted: quota_counter(),
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
        excubia_host::spawn_alerts(Arc::clone(&store), Arc::clone(&chat));

        // One mail surface for the host: the team's `mail.*` tools share its sends,
        // so a draft changed through a tool settles the owner's open question.
        let mail_logins = Arc::new(test_mail(
            &data_dir,
            Arc::clone(&chat),
            mail_transport(mail),
        ));
        let mail_sends = mail_logins.sends().expect("mail sends").clone();
        let mut team_service: Option<Arc<TeamService>> = None;
        let seat: Arc<dyn ChatSeat> = match seat {
            SeatKind::Fake => Arc::new(FakeSeat::replying(Arc::clone(&chat))),
            SeatKind::Silent => Arc::new(FakeSeat::silent()),
            SeatKind::Tools => Arc::new(ToolSeat),
            SeatKind::Cli { .. }
            | SeatKind::CliMissing
            | SeatKind::OpenCode { .. }
            | SeatKind::OpenCodeEngine { .. } => {
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
                    docs: Arc::new(DocTools::new(
                        Arc::clone(&craft),
                        Arc::clone(&chat),
                        excubia_mail::AuditLog::new(&data_dir.join(ACCOUNTS_DIR)),
                    )),
                    mail: Arc::new(test_mail_tools(
                        &data_dir,
                        Arc::clone(&chat),
                        &data_dir.join("workspaces"),
                        mail_sends.clone(),
                    )),
                })
                .expect("team service");
                team_service = Some(Arc::clone(&team));
                Arc::clone(&team) as Arc<dyn ChatSeat>
            }
        };

        let notices = Arc::new(PairingNotice::new(&data_dir, Arc::clone(&chat)));
        let alfred_engine = Arc::new(FakeEngine::default());
        let alfred = Arc::new(Switch::new(
            data_dir.clone(),
            Arc::clone(&notices),
            Arc::clone(&alfred_engine) as Arc<dyn Engine>,
        ));
        let actions: Arc<dyn AgentActions> = Arc::clone(&host) as Arc<dyn AgentActions>;
        let context = ApiContext::new(ApiConfig {
            store: Arc::clone(&store),
            actions,
            devices: Arc::clone(&devices),
            agents: catalog,
            chat: Arc::clone(&chat),
            seat: Arc::clone(&seat),
            hook_token: Arc::new(hook_token.clone()),
            mail: mail_logins,
            voice: Arc::new(VoiceService::new(voice)),
            alfred: Arc::clone(&alfred) as Arc<dyn AlfredBackend>,
            pairing_audit: notices,
            audit: Arc::new(AuditLog::new(&data_dir)),
            docs: craft as Arc<dyn excubia_api::DocumentPages>,
        })
        .with_host_pin(Some(host_pin));
        let pake_slot = Arc::clone(&context.pake_slot);
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
            host_pin,
            pake_slot,
            alfred,
            alfred_engine,
            cli,
            scratch,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Mint a single-use pairing code, exactly like `excubia-host pair-code` does.
    pub fn pair_code(&self) -> String {
        self.devices
            .issue_enrollment_code(excubia_auth::ENROLLMENT_TTL_SECONDS)
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
        let mut identity = excubia_test_support::DemoIdentity::generate("", name);
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
             X-Excubia-Hook-Token: {token}\r\nContent-Type: application/json\r\n\
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
            .append_user_message(&conversation.id, &format!("harness-{name}"), "hola", None)
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
             X-Excubia-Hook-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
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
