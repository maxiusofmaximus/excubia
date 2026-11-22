//! `GET` and `PUT /v1/pairing/policy`: whether a code-less pairing from the host's
//! tailnet owner is granted the reader set at once. Both need `ADMIN` (see `authz`);
//! the host CLI (`excubia-host pairing-policy`) sets the same value.

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::ApiContext;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyBody {
    tailnet_auto_accept: bool,
}

pub async fn get_pairing_policy(State(ctx): State<ApiContext>) -> Json<Value> {
    // The CLI may have changed the policy while this server runs.
    ctx.devices.reload_if_changed();
    Json(json!({"ok": true, "tailnet_auto_accept": ctx.devices.tailnet_auto_accept()}))
}

/// A body that is not exactly `{"tailnet_auto_accept": bool}` is refused as a whole: an unknown or a missing
/// field is `invalid_body`, not a guess.
pub async fn set_pairing_policy(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Response {
    let Ok(body) = serde_json::from_slice::<PolicyBody>(&body) else {
        return ApiError::invalid_body().into_response();
    };
    ctx.devices.reload_if_changed();
    if let Err(error) = ctx
        .devices
        .set_tailnet_auto_accept(body.tailnet_auto_accept)
    {
        return ApiError::internal(error).into_response();
    }
    ctx.audit.record(
        &AuditEvent::new("pairing_policy", Some(&verified.device_id), "set")
            .with("tailnet_auto_accept", body.tailnet_auto_accept.to_string()),
    );
    Json(json!({"ok": true, "tailnet_auto_accept": body.tailnet_auto_accept})).into_response()
}
