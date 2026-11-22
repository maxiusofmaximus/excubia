//! `POST /v1/pair/pake/start` and `/finish`: pairing with the short PAKE code (PAIRING-F2B3, `pairing.md` F2b).
//!
//! Both routes are public like `POST /v1/pair`: the code is the credential. Each message is bound to the TLS
//! connection it travels on (the RFC 9266 exporter of THAT request) and to the host's SPKI hash, so a relay with
//! two TLS sessions or another certificate cannot make the tags agree. A route that cannot read an exporter
//! (loopback, or no TLS identity) refuses before it touches the code.
//!
//! Order inside `start`: TLS binding, per-IP budget, body shape (JSON, base64, the device key, the message
//! length), and only then the code is taken. Anything that can be checked without the code is checked first, as
//! `DeviceStore::enroll` does, so a malformed request cannot burn a code the operator just printed. From the
//! take on, the code is spent whatever happens.
//!
//! Nothing secret is logged, audited or put in an error: not the code, the key, a tag or the exporter.

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Extension, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_auth::{p256_public_key_is_valid, Class, Curve};
use excubia_pake::{Role, Session, MESSAGE_LEN};
use excubia_pin::{unix_now, PakeLink};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::middleware::HookAuth;
use crate::routes::pair::{audit, pair_response};
use crate::routes::{device_sas, parse_body};
use crate::{ApiContext, PairOutcome, PairVia, PeerAddr, PAKE_CODE_TTL};

/// The same body cap as `POST /v1/pair` (`errors.md`).
const MAX_BODY_BYTES: usize = 8 * 1024;
/// Length of an HMAC-SHA256 confirmation tag.
const TAG_LEN: usize = 32;

/// The two public routes, mounted next to `POST /v1/pair` on every listener.
pub fn routes() -> Router<ApiContext> {
    Router::new()
        .route("/v1/pair/pake/start", post(start))
        .route("/v1/pair/pake/finish", post(finish))
}

/// The admin route. Only the loopback router mounts it, behind the hook guard.
pub fn admin_routes() -> Router<ApiContext> {
    Router::new().route("/v1/admin/pake-code", post(arm_code))
}

#[derive(Deserialize)]
struct StartBody {
    device_pubkey: String,
    message: String,
}

#[derive(Deserialize)]
struct FinishBody {
    tag: String,
}

/// A refusal with a stable, generic code, `{"detail": code}`.
fn refuse(status: StatusCode, code: &str) -> ApiError {
    ApiError {
        status,
        body: json!({ "detail": code }),
        retry_after: None,
    }
}

/// What both routes require before they look at the body: the exporter of this connection, the host's SPKI
/// hash, and budget in the `pake:<ip>` bucket (same class and size as `pair:<ip>`). Returns `(exporter, spki)`.
fn gate(ctx: &ApiContext, peer: &PeerAddr) -> Result<([u8; 32], [u8; 32]), ApiError> {
    let (Some(binding), Some(pin)) = (peer.binding, ctx.host_pin) else {
        return Err(refuse(StatusCode::FORBIDDEN, "pake_requires_tls"));
    };
    let key = format!("pake:{}", peer.addr.ip());
    ctx.limiter
        .allow(&key, Class::Write)
        .map_err(ApiError::slow_down)?;
    Ok((binding, *pin.as_bytes()))
}

fn decode(text: &str) -> Result<Vec<u8>, ApiError> {
    STANDARD.decode(text).map_err(|_| ApiError::invalid_body())
}

fn record(ctx: &ApiContext, peer: &PeerAddr, outcome: PairOutcome) {
    let ip = peer.addr.ip().to_string();
    audit(ctx, &ip, outcome, PairVia::Pake, None, None, None);
}

pub async fn start(
    State(ctx): State<ApiContext>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    body: Bytes,
) -> Response {
    match start_inner(&ctx, &peer, &body) {
        Ok(answer) => answer.into_response(),
        Err(error) => error.into_response(),
    }
}

fn start_inner(ctx: &ApiContext, peer: &PeerAddr, body: &Bytes) -> Result<Json<Value>, ApiError> {
    let (binding, spki) = gate(ctx, peer)?;
    if body.len() > MAX_BODY_BYTES {
        return Err(ApiError::body_too_large());
    }
    let payload: StartBody = parse_body(body).map_err(|_| ApiError::invalid_body())?;
    let device_pubkey = decode(&payload.device_pubkey)?;
    let message = decode(&payload.message)?;
    if !p256_public_key_is_valid(&device_pubkey) || message.len() != MESSAGE_LEN {
        return Err(ApiError::invalid_body());
    }

    // From here on the code is spent, right or wrong.
    let Some((code, host_id)) = ctx.pake_slot.take() else {
        record(ctx, peer, PairOutcome::PakeFailed);
        return Err(refuse(StatusCode::GONE, "no_live_code"));
    };
    let started = Session::start(Role::Host, &code, &host_id, &device_pubkey);
    drop(code);
    let (session, own_message) = started.map_err(ApiError::internal)?;
    let Ok(key) = session.finish(&message) else {
        record(ctx, peer, PairOutcome::PakeFailed);
        return Err(refuse(StatusCode::BAD_REQUEST, "pake_invalid_message"));
    };
    let tag = key
        .confirm_tag(Role::Host, &binding, &spki)
        .map_err(ApiError::internal)?;
    ctx.pake_slot.hold_pending(key, device_pubkey);
    record(ctx, peer, PairOutcome::PakeStarted);
    Ok(Json(json!({
        "message": STANDARD.encode(own_message),
        "tag": STANDARD.encode(tag),
    })))
}

pub async fn finish(
    State(ctx): State<ApiContext>,
    ConnectInfo(peer): ConnectInfo<PeerAddr>,
    body: Bytes,
) -> Response {
    match finish_inner(&ctx, &peer, &body) {
        Ok(answer) => answer.into_response(),
        Err(error) => error.into_response(),
    }
}

fn finish_inner(ctx: &ApiContext, peer: &PeerAddr, body: &Bytes) -> Result<Json<Value>, ApiError> {
    let (binding, spki) = gate(ctx, peer)?;
    if body.len() > MAX_BODY_BYTES {
        return Err(ApiError::body_too_large());
    }
    let payload: FinishBody = parse_body(body).map_err(|_| ApiError::invalid_body())?;
    let tag = decode(&payload.tag)?;
    if tag.len() != TAG_LEN {
        return Err(ApiError::invalid_body());
    }

    // One use: the session is gone after this call, whatever the tag says.
    let Some(pending) = ctx.pake_slot.take_pending() else {
        record(ctx, peer, PairOutcome::PakeFailed);
        return Err(refuse(StatusCode::GONE, "pake_session_gone"));
    };
    if pending
        .key
        .verify_confirm(Role::Device, &binding, &spki, &tag)
        .is_err()
    {
        record(ctx, peer, PairOutcome::PakeFailed);
        return Err(refuse(StatusCode::FORBIDDEN, "confirmation_failed"));
    }

    // The device is pending, as after `/v1/pair`: the SAS approval of F1 is the only way to give it permissions.
    // The owner is empty on purpose: the code, not a tailnet identity, vouched for this device.
    ctx.devices.reload_if_changed();
    let public_key = STANDARD.encode(&pending.device_pubkey);
    let device = ctx
        .devices
        .add_device(Curve::P256, &public_key, None, "")
        .map_err(ApiError::internal)?;
    let recovery = ctx
        .devices
        .issue_recovery_code(&device.device_id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::internal("device vanished while pairing"))?;
    let ip = peer.addr.ip().to_string();
    audit(
        ctx,
        &ip,
        PairOutcome::Paired,
        PairVia::Pake,
        Some(&device.device_id),
        None,
        device_sas(ctx.host_pin.as_ref(), &device),
    );
    Ok(pair_response(&device, &recovery))
}

/// `POST /v1/admin/pake-code`: arms the one PAKE code and shows it. Loopback listener only, behind the hook
/// guard, and only the host token opens it: a per-run agent token must not be able to mint pairing codes.
/// Answers `code`, `expires_in` and, for the QR (F3a), `expires_at` (unix) and `uri` (a `PakeLink`, no address).
pub async fn arm_code(
    State(ctx): State<ApiContext>,
    Extension(auth): Extension<HookAuth>,
) -> Response {
    if !matches!(auth, HookAuth::Host) {
        return ApiError::forbidden_code("host_token_required").into_response();
    }
    // The host id is the SPKI hash the device sees; without a TLS identity there is nothing to pair with.
    let Some(pin) = ctx.host_pin else {
        return refuse(StatusCode::CONFLICT, "pake_unavailable").into_response();
    };
    match ctx
        .pake_slot
        .arm(PAKE_CODE_TTL, &hex::encode(pin.as_bytes()))
    {
        Ok(shown) => {
            // The URI holds the live code: it is a secret like the code, and goes nowhere but this answer.
            let expires_at = unix_now() + PAKE_CODE_TTL.as_secs() as i64;
            let link = PakeLink {
                code: shown.reveal().to_owned(),
                host: pin,
                expires_at,
            };
            let mut response = Json(json!({
                "code": shown.reveal(),
                "expires_in": PAKE_CODE_TTL.as_secs(),
                "expires_at": expires_at,
                "uri": link.to_uri(),
            }))
            .into_response();
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => ApiError::internal(error).into_response(),
    }
}
