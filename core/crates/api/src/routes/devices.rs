//! Device management: the `ADMIN` surface.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::ApiContext;

/// Every paired device, for the "My devices" screen. Public material only:
/// never a public key, never an FCM token.
pub async fn list_devices(State(ctx): State<ApiContext>) -> Json<Value> {
    let devices: Vec<_> = ctx
        .devices
        .list()
        .iter()
        .map(|device| device.to_public())
        .collect();
    Json(json!({"ok": true, "devices": devices}))
}

/// Revoke one device; its next signed request is a `401` and any stream it
/// holds closes at once. Revocation marks rather than deletes.
pub async fn revoke_device(
    State(ctx): State<ApiContext>,
    Path(device_id): Path<String>,
) -> Response {
    match ctx.devices.revoke(&device_id) {
        Ok(true) => {
            // Streams watch this channel so a revoke closes them immediately.
            let _ = ctx.revocations.send(device_id.clone());
            Json(json!({"ok": true, "device_id": device_id, "revoked": true})).into_response()
        }
        Ok(false) => ApiError::no_such_device().into_response(),
        Err(error) => ApiError::internal(error).into_response(),
    }
}
