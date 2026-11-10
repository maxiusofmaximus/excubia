//! Device management: the `ADMIN` surface.

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_core_auth::{Device, GrantOutcome, Permission, VerifiedDevice};
use excubia_core_pin::{sas, Fingerprint};
use serde::Deserialize;
use serde_json::{json, Value};

use super::parse_body;
use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::{ApiContext, PairOutcome, PairVia, PairingAttempt, PeerAddr};

/// The verification string of a device as the phone computes it: the host pin
/// and the SEC1 key the device sent. `None` when this host has no pin.
pub fn device_sas(pin: Option<&Fingerprint>, device: &Device) -> Option<String> {
    let key = STANDARD.decode(&device.public_key).ok()?;
    Some(sas(pin?, &key))
}

/// Every paired device, for the "My devices" screen. Public material only:
/// never a public key, never an FCM token. A pending device (no permission, not
/// revoked) also carries its `sas`, the string the Master compares with the phone's.
pub async fn list_devices(State(ctx): State<ApiContext>) -> Json<Value> {
    let devices: Vec<Value> = ctx
        .devices
        .list()
        .iter()
        .map(|device| {
            let mut public = json!(device.to_public());
            if device.permissions.is_empty() && !device.revoked {
                if let Some(text) = device_sas(ctx.host_pin.as_ref(), device) {
                    public["sas"] = json!(text);
                }
            }
            public
        })
        .collect();
    Json(json!({"ok": true, "devices": devices}))
}

#[derive(Deserialize)]
struct GrantBody {
    permissions: Vec<String>,
}

/// Grant a pending device its first permissions. `ADMIN` is never granted here
/// (only the host CLI does that), and a device that already holds access is
/// refused: widening it stays a host-side act.
pub async fn grant_device(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    ConnectInfo(PeerAddr { addr, .. }): ConnectInfo<PeerAddr>,
    Path(device_id): Path<String>,
    body: Bytes,
) -> Response {
    let body: GrantBody = match parse_body(&body) {
        Ok(body) => body,
        Err(error) => return error.into_response(),
    };
    let permissions = match parse_permissions(&body.permissions) {
        Ok(permissions) => permissions,
        Err(error) => return error.into_response(),
    };
    if device_id == verified.device_id {
        return ApiError::action("cannot_grant_self", "a device cannot grant itself")
            .into_response();
    }

    // The trust store can be rewritten by the CLI while this server runs.
    ctx.devices.reload_if_changed();
    let granted = match ctx.devices.grant_pending(&device_id, &permissions) {
        Ok(GrantOutcome::Granted(device)) => *device,
        Ok(GrantOutcome::NoSuchDevice) => return ApiError::no_such_device().into_response(),
        Ok(GrantOutcome::NotPending) => {
            return ApiError::coded(
                StatusCode::CONFLICT,
                "not_pending",
                "the device already has permissions; only a pending device is granted here",
            )
            .into_response()
        }
        Err(error) => return ApiError::internal(error).into_response(),
    };

    let mut attempt = PairingAttempt::new(
        PairOutcome::Granted,
        PairVia::Admin,
        Some(&granted.device_id),
        granted.label.as_deref(),
        &addr.ip().to_string(),
    );
    attempt.granted_by = Some(verified.device_id);
    attempt.permissions = permissions
        .iter()
        .map(|permission| permission.label().to_string())
        .collect();
    ctx.pairing_audit.record(&attempt);
    ctx.audit.record(
        &AuditEvent::new("devices.grant", Some(&granted.device_id), "granted")
            .with("by", attempt.granted_by.clone().unwrap_or_default())
            .with("permissions", attempt.permissions.join(",")),
    );
    Json(json!(granted.to_public())).into_response()
}

/// The permissions a grant names: at least one, each a known label (case does
/// not matter), and never `admin`.
fn parse_permissions(labels: &[String]) -> Result<Vec<Permission>, ApiError> {
    if labels.is_empty() {
        return Err(ApiError::invalid_field(
            "invalid_body",
            "permissions",
            "name at least one permission",
        ));
    }
    let mut permissions = Vec::with_capacity(labels.len());
    for label in labels {
        match Permission::from_label(&label.to_ascii_lowercase()) {
            Some(Permission::Admin) => {
                return Err(ApiError::action(
                    "admin_not_grantable",
                    "ADMIN is not granted over HTTP; use `host devices grant --permission admin`",
                ))
            }
            Some(permission) => permissions.push(permission),
            None => {
                return Err(ApiError::invalid_field(
                    "invalid_body",
                    "permissions",
                    format!("unknown permission: {label}"),
                ))
            }
        }
    }
    permissions.sort();
    permissions.dedup();
    Ok(permissions)
}

/// Revoke one device; its next signed request is a `401` and any stream it
/// holds closes at once. Revocation marks rather than deletes.
pub async fn revoke_device(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(device_id): Path<String>,
) -> Response {
    match ctx.devices.revoke(&device_id) {
        Ok(true) => {
            // Streams watch this channel so a revoke closes them immediately.
            let _ = ctx.revocations.send(device_id.clone());
            ctx.audit.record(
                &AuditEvent::new("devices.revoke", Some(&device_id), "revoked")
                    .with("by", verified.device_id),
            );
            Json(json!({"ok": true, "device_id": device_id, "revoked": true})).into_response()
        }
        Ok(false) => ApiError::no_such_device().into_response(),
        Err(error) => ApiError::internal(error).into_response(),
    }
}
