//! Local HTTP + SSE API for the Excubia core: the Rust implementation of
//! `docs/contracts/{api,state,errors}.md`, with the signed-request
//! authentication and authorization of `docs/contracts/{auth,pairing}.md`.
//!
//! The caller binds the listener; this crate only serves it. Binding loopback
//! by default is the host's job.

mod agenda_api;
mod alert_api;
mod alfred;
mod audit;
mod catalog;
mod contact_api;
mod doc_pages;
mod error;
mod hire_api;
mod mail;
mod middleware;
mod note_api;
mod pairing_audit;
mod pake_slot;
mod recipe_api;
pub mod role_catalog;
mod routes;
mod schedule_api;
mod sse;
mod tls;
mod validate;
pub mod voice;

pub use alfred::{AlfredBackend, BackendStatus, EngineListing, Requested};
pub use audit::{AuditEvent, AuditSink};
pub use catalog::{AgentCatalog, AgentSpec};
pub use doc_pages::{DocTool, DocumentPages, PageError, PageSource, PreviewLocks, RenderedPage};
pub use error::ApiError;
pub use mail::{
    DraftEdit, MailAccount, MailActions, MailDraft, MailDraftAttachment, MailFailure,
    MailLoginStart, MailLoginStatus, SendAnswer, SendFailure, SendStatus,
};
pub use pairing_audit::{PairOutcome, PairVia, PairingAttempt, PairingAudit};
pub use pake_slot::{PakeSlot, PendingPake, ShownCode, PAKE_CODE_TTL, PAKE_PENDING_TTL};
pub use routes::{device_sas, RestoreRefusal};
pub use tls::TlsListener;
pub use validate::{clamp_read_lines, validate_keys, validate_pane_id, validate_prompt};

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::connect_info::Connected;
use axum::routing::{get, post, put};
use axum::serve::{IncomingStream, Listener};
use axum::{middleware as axum_middleware, Router};
use excubia_auth::{DeviceStore, RateLimiter, RequestVerifier, StreamRegistry};
use excubia_chat::{ChatSeat, ChatStore};
use excubia_pin::Fingerprint;
use excubia_state::StateStore;
use serde_json::Value;
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
        spec: &AgentSpec,
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

    /// Save the folder of a live agent before its turn; returns the point's id. The
    /// caller goes on with the turn whatever the answer, and records a failure.
    async fn begin_turn(&self, pane_id: &str) -> Result<String, String>;

    /// The restore points of an agent, newest first, without their folder paths.
    async fn restore_points(&self, pane_id: &str) -> Result<Value, RestoreRefusal>;

    /// What restoring a point would change, and what the folder leaves out.
    async fn restore_changes(&self, point_id: &str) -> Result<Value, RestoreRefusal>;

    /// Put a point's folder back as it was. The state before is kept as a point.
    async fn restore_point(&self, point_id: &str) -> Result<Value, RestoreRefusal>;
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
    /// The mail logins and accounts the host keeps (MAIL1D §1).
    pub mail: Arc<dyn MailActions>,
    pub voice: Arc<voice::VoiceService>,
    /// The engine that answers Alfred, and its switch (`/v1/alfred/backend`).
    pub alfred: Arc<dyn AlfredBackend>,
    /// Where each `POST /v1/pair` decision is recorded.
    pub pairing_audit: Arc<dyn PairingAudit>,
    /// The host's record of sensitive actions (`audit.jsonl`).
    pub audit: Arc<dyn AuditSink>,
    /// In-process revocation notifications, so a revoke closes open streams
    /// immediately instead of on their next tick.
    pub revocations: broadcast::Sender<String>,
    /// The host's active certificate pin: the input of a pending device's SAS.
    /// `None` when there is no certificate to read, and then no SAS is shown.
    pub host_pin: Option<Fingerprint>,
    /// The renderer of the document previews (DOC5B): the host's document CLIs.
    pub docs: Arc<dyn DocumentPages>,
    /// One lock per attachment, so a page renders once for two requests.
    pub preview_locks: Arc<PreviewLocks>,
    /// The PAKE pairing code, in memory only. It is never written to the trust store and `POST /v1/pair` does not
    /// read it.
    pub pake_slot: Arc<PakeSlot>,
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
    pub mail: Arc<dyn MailActions>,
    pub voice: Arc<voice::VoiceService>,
    pub alfred: Arc<dyn AlfredBackend>,
    pub pairing_audit: Arc<dyn PairingAudit>,
    pub audit: Arc<dyn AuditSink>,
    pub docs: Arc<dyn DocumentPages>,
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
            mail: config.mail,
            voice: config.voice,
            alfred: config.alfred,
            pairing_audit: config.pairing_audit,
            audit: config.audit,
            revocations,
            host_pin: None,
            pake_slot: Arc::new(PakeSlot::new()),
            docs: config.docs,
            preview_locks: Arc::new(PreviewLocks::default()),
        }
    }

    pub fn with_host_pin(mut self, pin: Option<Fingerprint>) -> Self {
        self.host_pin = pin;
        self
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
        .route("/v1/catalog/default", post(routes::catalog_set_default))
        .route("/v1/catalog/{name}/fix", post(routes::catalog_fix))
        .route("/v1/doc-tools", get(routes::doc_tools))
        .route("/v1/roles", get(routes::role_catalog))
        .route(
            "/v1/hires",
            get(routes::list_hires).post(routes::create_hire),
        )
        .route("/v1/hires/{hire_id}/update", post(routes::update_hire))
        .route("/v1/hires/{hire_id}/dismiss", post(routes::dismiss_hire))
        .route(
            "/v1/onboarding",
            get(routes::onboarding_state).post(routes::complete_onboarding),
        )
        .route("/v1/team/summary", get(routes::team_summary))
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
        .route(
            "/v1/recipes",
            get(routes::list_recipes).post(routes::create_recipe),
        )
        .route("/v1/recipes/draft", post(routes::draft_recipe))
        .route(
            "/v1/recipes/{recipe_id}",
            put(routes::update_recipe).delete(routes::delete_recipe),
        )
        .route("/v1/recipes/{recipe_id}/run", post(routes::run_recipe))
        .route(
            "/v1/schedules",
            get(routes::list_schedules).post(routes::create_schedule),
        )
        .route(
            "/v1/schedules/{schedule_id}",
            put(routes::update_schedule).delete(routes::delete_schedule),
        )
        .route(
            "/v1/schedules/{schedule_id}/pause",
            post(routes::pause_schedule),
        )
        .route(
            "/v1/schedules/{schedule_id}/resume",
            post(routes::resume_schedule),
        )
        .route(
            "/v1/schedules/{schedule_id}/run_now",
            post(routes::run_schedule_now),
        )
        .route(
            "/v1/schedules/{schedule_id}/runs",
            get(routes::schedule_runs),
        )
        .route("/v1/plans", get(routes::list_plans))
        .route("/v1/plans/{plan_id}/approve", post(routes::approve_plan))
        .route("/v1/plans/{plan_id}/reject", post(routes::reject_plan))
        .route(
            "/v1/plans/{plan_id}/request_changes",
            post(routes::request_plan_changes),
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
            "/v1/chat/conversations/{conversation_id}/stop",
            post(routes::chat_stop_conversation),
        )
        .route(
            "/v1/chat/conversations",
            get(routes::chat_conversations).post(routes::chat_create_conversation),
        )
        .route(
            "/v1/chat/conversations/counts",
            get(routes::chat_conversation_counts),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}",
            get(routes::chat_conversation).delete(routes::chat_delete_conversation),
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
            "/v1/chat/conversations/{conversation_id}/attachments",
            post(routes::chat_create_attachment),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}",
            get(routes::chat_attachment).delete(routes::chat_delete_attachment),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}/chunks",
            post(routes::chat_upload_chunk),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}/complete",
            post(routes::chat_complete_attachment),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}/content",
            get(routes::chat_attachment_content),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}/preview",
            get(routes::chat_attachment_preview),
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
        .route("/v1/devices/{device_id}/grant", post(routes::grant_device))
        .route("/v1/mail/login/start", post(routes::mail_login_start))
        .route("/v1/mail/login/status", get(routes::mail_login_status))
        .route("/v1/mail/accounts", get(routes::mail_accounts))
        .route("/v1/mail/drafts/{draft_id}", get(routes::mail_draft_review))
        .route(
            "/v1/mail/drafts/{draft_id}/update",
            post(routes::mail_draft_update),
        )
        .route(
            "/v1/mail/drafts/{draft_id}/send",
            post(routes::mail_send_draft),
        )
        .route("/v1/mail/sends/{send_id}", get(routes::mail_send_status))
        .route(
            "/v1/mail/sends/{send_id}/cancel",
            post(routes::mail_send_cancel),
        )
        .route("/v1/mail/send/enable", post(routes::mail_send_enable_start))
        .route(
            "/v1/mail/send/enable/status",
            get(routes::mail_send_enable_status),
        )
        .route("/v1/mail/send/disable", post(routes::mail_send_disable))
        .route(voice::TRANSCRIBE_PATH, post(routes::transcribe_voice))
        .route(
            "/v1/alfred/backend",
            get(routes::alfred_status).post(routes::alfred_request),
        )
        .route("/v1/restore/points", get(routes::restore_points))
        .route(
            "/v1/restore/points/{point_id}/files",
            get(routes::restore_changes),
        )
        .route(
            "/v1/restore/points/{point_id}/restore",
            post(routes::restore_point),
        )
        .merge(routes::agenda_routes())
        .route_layer(axum_middleware::from_fn_with_state(
            ctx.clone(),
            middleware::authenticate,
        ));
    let public = Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/pair", post(routes::pair))
        .merge(routes::pake_routes());
    public.merge(protected)
}

/// The hook surface, with its own local guard. Only the host's own listener
/// mounts it: agents report to loopback, never to the LAN.
fn hook_routes(ctx: &ApiContext) -> Router<ApiContext> {
    Router::new()
        .route("/v1/hooks/report", post(routes::hook_report))
        // Arms the PAKE pairing code: loopback only, and the host token alone opens it.
        .merge(routes::pake_admin_routes())
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

/// The address a device connected from, and what its connection can bind a message to. It is this crate's type,
/// so each listener can supply it through its own `Connected` impl (see `tls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr {
    pub addr: SocketAddr,
    /// The RFC 9266 `tls-exporter` value of this connection. `None` when there is no TLS (loopback) or the
    /// exporter could not be read: whatever needs a channel binding must refuse then.
    pub binding: Option<[u8; 32]>,
}

impl Connected<IncomingStream<'_, TcpListener>> for PeerAddr {
    fn connect_info(stream: IncomingStream<'_, TcpListener>) -> Self {
        Self {
            addr: *stream.remote_addr(),
            binding: None,
        }
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
