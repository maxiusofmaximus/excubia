//! `POST /v1/pair`: the one unauthenticated write.
//!
//! Gated by a host-printed single-use code, a per-device recovery code, or the
//! tailnet owner of the source address — never a static secret. A supplied
//! code that fails answers a uniform `403 pairing_rejected`; a missing code
//! answers a `{code, message}` object that names the working path and reveals
//! nothing about owners.

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::{is_host_owner, tailnet_owner_of, Class, Curve};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::{ApiContext, PeerAddr};

/// The enrollment-body cap of `errors.md` (8 KiB).
const MAX_PAIR_BODY_BYTES: usize = 8 * 1024;
const MAX_PUBLIC_KEY_CHARS: usize = 512;
const MAX_LABEL_CHARS: usize = 64;
const MAX_CODE_CHARS: usize = 64;

#[derive(Deserialize)]
struct PairBody {
    curve: String,
    public_key: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

pub async fn pair(
    State(ctx): State<ApiContext>,
    ConnectInfo(PeerAddr(addr)): ConnectInfo<PeerAddr>,
    body: Bytes,
) -> Response {
    if body.len() > MAX_PAIR_BODY_BYTES {
        return ApiError::body_too_large().into_response();
    }
    // Every model failure on this route is the uniform `400 invalid_body`.
    let payload: PairBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(_) => return ApiError::invalid_body().into_response(),
    };
    let Some(curve) = Curve::from_label(&payload.curve) else {
        return ApiError::invalid_body().into_response();
    };
    if payload.public_key.is_empty()
        || payload.public_key.len() > MAX_PUBLIC_KEY_CHARS
        || payload
            .label
            .as_deref()
            .is_some_and(|label| label.len() > MAX_LABEL_CHARS)
        || payload
            .code
            .as_deref()
            .is_some_and(|code| code.len() > MAX_CODE_CHARS)
    {
        return ApiError::invalid_body().into_response();
    }

    let ip = addr.ip().to_string();
    if let Err(retry_after) = ctx.limiter.allow(&format!("pair:{ip}"), Class::Write) {
        return ApiError::slow_down(retry_after).into_response();
    }

    // The trust store can be rewritten by the CLI while this server runs.
    ctx.devices.reload_if_changed();
    let owner = tailnet_owner_of(&ip).unwrap_or_default();
    let code = payload.code.unwrap_or_default();

    let device = if code.is_empty() {
        if !is_host_owner(&ip) {
            return empty_code_refusal(&ip).into_response();
        }
        match ctx
            .devices
            .add_device(curve, &payload.public_key, payload.label.clone(), &owner)
        {
            Ok(device) => device,
            Err(error) => return ApiError::internal(error).into_response(),
        }
    } else {
        match ctx.devices.enroll(
            &code,
            curve,
            &payload.public_key,
            payload.label.clone(),
            &owner,
        ) {
            Ok(device) => device,
            Err(_) => {
                // A recovery code re-keys its device; anything else is the
                // uniform refusal.
                match ctx.devices.use_recovery_code(
                    &code,
                    curve,
                    &payload.public_key,
                    payload.label.clone(),
                    &owner,
                ) {
                    Ok((device, recovery)) => {
                        return pair_response(&device, &recovery).into_response()
                    }
                    Err(_) => return rejected().into_response(),
                }
            }
        }
    };

    match ctx.devices.issue_recovery_code(&device.device_id) {
        Ok(Some(recovery)) => pair_response(&device, &recovery).into_response(),
        Ok(None) => ApiError::internal("device vanished while pairing").into_response(),
        Err(error) => ApiError::internal(error).into_response(),
    }
}

/// Uniform refusal for a supplied code: the caller is never told which check
/// failed, the same oracle reasoning as the uniform `401`/`403`.
fn rejected() -> ApiError {
    ApiError {
        status: axum::http::StatusCode::FORBIDDEN,
        body: json!({"detail": "pairing_rejected"}),
        retry_after: None,
    }
}

/// With no code there is only the owner check, and the caller knows it sent
/// none, so the refusal names the working path. It never reports whose account
/// owns an address.
fn empty_code_refusal(ip: &str) -> ApiError {
    let message = if ip == "127.0.0.1" || ip == "::1" {
        "no pairing code was provided and this request arrived from a local \
         address, so it cannot be matched to the host's tailnet owner (a local \
         proxy makes every request look local); run `host pair-code` on the \
         host and enter the code"
    } else {
        "no pairing code was provided and this address is not the host's \
         tailnet owner; run `host pair-code` on the host and enter the code"
    };
    ApiError {
        status: axum::http::StatusCode::FORBIDDEN,
        body: json!({"detail": {"code": "pairing_rejected", "message": message}}),
        retry_after: None,
    }
}

/// The recovery code is the one secret here and is returned exactly once:
/// only a hash is stored, so this is the only moment it can be shown.
fn pair_response(device: &hn_core_auth::Device, recovery: &str) -> Json<Value> {
    let mut permissions: Vec<String> = device
        .permissions
        .iter()
        .map(|permission| permission.label().to_string())
        .collect();
    permissions.sort();
    Json(json!({
        "ok": true,
        "device_id": device.device_id,
        "curve": device.curve.label(),
        "label": device.label,
        "permissions": permissions,
        "owner_id": device.owner_id,
        "recovery_code": recovery,
    }))
}
