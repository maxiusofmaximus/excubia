//! Local HTTP + SSE API for the herdr-notify core: the Rust implementation of
//! `docs/contracts/{api,state,errors}.md`, with the signed-request
//! authentication and authorization of `docs/contracts/{auth,pairing}.md`.
//!
//! The caller binds the listener; this crate only serves it. Binding loopback
//! by default is the host's job.

mod catalog;
mod error;
mod middleware;
mod routes;
mod sse;
mod tls;
mod validate;

pub use catalog::{AgentCatalog, AgentSpec};
pub use error::ApiError;
pub use tls::TlsListener;
pub use validate::{clamp_read_lines, validate_keys, validate_pane_id, validate_prompt};

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::connect_info::Connected;
use axum::routing::{get, post};
use axum::serve::{IncomingStream, Listener};
use axum::{middleware as axum_middleware, Router};
use hn_core_auth::{DeviceStore, RateLimiter, RequestVerifier, StreamRegistry};
use hn_core_chat::{ChatSeat, ChatStore};
use hn_core_state::StateStore;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// The states a hook may report. `done` is not one: it is derived by the
/// state store from `idle` + "not seen".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookState {
    Idle,
    Working,
    Blocked,
    Unknown,
}

impl HookState {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "blocked" => Some(Self::Blocked),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// What `start_agent` produced.
#[derive(Clone, Debug)]
pub struct StartedAgent {
    pub pane_id: String,
    pub workspace_id: String,
    pub agent: String,
    pub status: String,
}

/// What the API can do to a supervised agent. Implemented by the host.
/// A failure is the host's own refusal (a `409`), not a validation error.
#[async_trait]
pub trait AgentActions: Send + Sync + 'static {
    fn respond(&self, pane_id: &str, text: &str) -> Result<(), String>;
    fn send_keys(&self, pane_id: &str, keys: &[String], confirm: bool) -> Result<(), String>;
    fn read(&self, pane_id: &str, lines: u16) -> Result<String, String>;

    /// Start an agent from the host's own catalogue. The command comes from
    /// the host's configuration, never from the request.
    async fn start_agent(
        &self,
        command: &str,
        args: &[String],
        cwd: &Path,
        label: Option<&str>,
    ) -> Result<StartedAgent, String>;

    /// Record a hook report and return the effective status.
    async fn report_agent(
        &self,
        pane_id: &str,
        state: HookState,
        message: Option<&str>,
    ) -> Result<String, String>;

    /// Close a live agent's process and wait for its pane to leave the state
    /// list. The caller has already checked that the pane exists and is not
    /// restored.
    async fn stop_agent(&self, pane_id: &str) -> Result<(), String>;
}

/// Everything a route needs.
#[derive(Clone)]
pub struct ApiContext {
    pub store: Arc<StateStore>,
    pub actions: Arc<dyn AgentActions>,
    pub devices: Arc<DeviceStore>,
    pub verifier: Arc<RequestVerifier>,
    pub limiter: Arc<RateLimiter>,
    pub streams: Arc<StreamRegistry>,
    pub agents: Arc<AgentCatalog>,
    /// Persistent conversations, messages and sections.
    pub chat: Arc<ChatStore>,
    /// The agent seat that receives durable turns (CORE6B).
    pub seat: Arc<dyn ChatSeat>,
    /// The host token hooks authenticate with (loopback only).
    pub hook_token: Arc<String>,
    /// In-process revocation notifications, so a revoke closes open streams
    /// immediately instead of on their next tick.
    pub revocations: broadcast::Sender<String>,
}

/// The pieces the API is built from.
pub struct ApiConfig {
    pub store: Arc<StateStore>,
    pub actions: Arc<dyn AgentActions>,
    pub devices: Arc<DeviceStore>,
    pub agents: Arc<AgentCatalog>,
    pub chat: Arc<ChatStore>,
    pub seat: Arc<dyn ChatSeat>,
    pub hook_token: Arc<String>,
}

impl ApiContext {
    pub fn new(config: ApiConfig) -> Self {
        let (revocations, _) = broadcast::channel(64);
        Self {
            verifier: Arc::new(RequestVerifier::new(Arc::clone(&config.devices))),
            store: config.store,
            actions: config.actions,
            devices: config.devices,
            limiter: Arc::new(RateLimiter::new()),
            streams: Arc::new(StreamRegistry::new()),
            agents: config.agents,
            chat: config.chat,
            seat: config.seat,
            hook_token: config.hook_token,
            revocations,
        }
    }
}

/// Every route a device may reach: the public ones and the signed ones. Each
/// listener mounts this, so the LAN TLS listener never sees a hook route.
fn client_routes(ctx: &ApiContext) -> Router<ApiContext> {
    let protected = Router::new()
        .route("/v1/state", get(routes::state))
        .route("/v1/events", get(routes::events))
        .route("/v1/agents/{pane_id}/respond", post(routes::respond))
        .route("/v1/agents/{pane_id}/keys", post(routes::keys))
        .route("/v1/agents/{pane_id}/read", post(routes::read))
        .route("/v1/agents/{pane_id}/seen", post(routes::seen))
        .route("/v1/agents/{pane_id}/stop", post(routes::stop))
        .route("/v1/catalog", get(routes::catalog))
        .route("/v1/workspaces", post(routes::create_workspace))
        .route("/v1/chat/bootstrap", get(routes::chat_bootstrap))
        .route(
            "/v1/chat/delegations/{delegation_id}",
            get(routes::chat_delegation),
        )
        .route(
            "/v1/chat/delegations/{delegation_id}/cancel",
            post(routes::chat_cancel_delegation),
        )
        .route("/v1/chat/approvals", get(routes::chat_approvals))
        .route(
            "/v1/chat/approvals/settings",
            get(routes::chat_approval_settings).post(routes::chat_change_approval_settings),
        )
        .route(
            "/v1/chat/approvals/{approval_id}/decision",
            post(routes::chat_decide_approval),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/cancel",
            post(routes::chat_cancel_conversation),
        )
        .route(
            "/v1/chat/conversations",
            get(routes::chat_conversations).post(routes::chat_create_conversation),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}",
            get(routes::chat_conversation),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/update",
            post(routes::chat_update_conversation),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/timeline",
            get(routes::chat_timeline),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/messages",
            get(routes::chat_messages).post(routes::chat_send_message),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/read",
            post(routes::chat_read),
        )
        .route(
            "/v1/chat/sections",
            get(routes::chat_sections).post(routes::chat_upsert_section),
        )
        .route("/v1/devices", get(routes::list_devices))
        .route(
            "/v1/devices/{device_id}/revoke",
            post(routes::revoke_device),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            ctx.clone(),
            middleware::authenticate,
        ));
    let public = Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/pair", post(routes::pair));
    public.merge(protected)
}

/// The hook surface, with its own local guard. Only the host's own listener
/// mounts it: agents report to loopback, never to the LAN.
fn hook_routes(ctx: &ApiContext) -> Router<ApiContext> {
    Router::new()
        .route("/v1/hooks/report", post(routes::hook_report))
        // The approval surface of the agent's own hook: it opens, polls and
        // burns an approval with the host token, never with a device one.
        .route("/v1/chat/approvals", post(routes::chat_create_approval))
        .route(
            "/v1/chat/approvals/{approval_id}",
            get(routes::chat_approval_state),
        )
        .route(
            "/v1/chat/approvals/{approval_id}/consume",
            post(routes::chat_consume_approval),
        )
        .route_layer(axum_middleware::from_fn_with_state(
            ctx.clone(),
            middleware::hook_guard,
        ))
}

/// The loopback listener's router: device routes and hook routes.
pub fn router(ctx: ApiContext) -> Router {
    client_routes(&ctx).merge(hook_routes(&ctx)).with_state(ctx)
}

/// The LAN listener's router: device routes only.
pub fn client_router(ctx: ApiContext) -> Router {
    client_routes(&ctx).with_state(ctx)
}

/// The address a device connected from. It is this crate's type, so each
/// listener can supply it through its own `Connected` impl (see `tls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr(pub SocketAddr);

impl Connected<IncomingStream<'_, TcpListener>> for PeerAddr {
    fn connect_info(stream: IncomingStream<'_, TcpListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

/// Serve the router on an already-bound listener. Peer addresses are captured
/// because the pairing rate limit, the owner check and the hook guard need
/// the source IP.
pub async fn serve(listener: TcpListener, ctx: ApiContext) -> std::io::Result<()> {
    serve_app(listener, router(ctx)).await
}

/// Serve the device routes only, on the LAN TLS listener.
pub async fn serve_client(listener: TlsListener, ctx: ApiContext) -> std::io::Result<()> {
    serve_app(listener, client_router(ctx)).await
}

async fn serve_app<L>(listener: L, app: Router) -> std::io::Result<()>
where
    L: Listener,
    L::Addr: std::fmt::Debug,
    for<'a> PeerAddr: Connected<IncomingStream<'a, L>>,
{
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<PeerAddr>(),
    )
    .await
}

/// The internal MCP tool surface. It has no device signature: it is only ever
/// bound to loopback and every call must carry the per-run token. Keeping it
/// off the public listener is what makes the token a second line of defense
/// instead of the only one.
pub fn internal_router(ctx: ApiContext) -> Router {
    Router::new()
        .route("/internal/chat/tools", post(routes::chat_tool))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .with_state(ctx)
}

/// Bind-time guard: refuse to serve the internal surface on a non-loopback
/// address, even if a caller made a mistake.
pub async fn serve_internal(
    listener: tokio::net::TcpListener,
    ctx: ApiContext,
) -> std::io::Result<()> {
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the internal MCP listener must be loopback",
        ));
    }
    axum::serve(listener, internal_router(ctx).into_make_service()).await
}
