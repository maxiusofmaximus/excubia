//! `GET /v1/vault/status`, `POST /v1/vault/init` and `POST /v1/vault/finish`: the vault of the chat database
//! (`docs/rfc/vault-chatdb-encryption.md`, V2). All three need `ADMIN`; the host does the work through
//! `VaultControl`, which keeps the Credential Manager out of this crate.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::audit::AuditEvent;
use crate::error::ApiError;
use crate::{ApiContext, VaultRefusal, VaultStatus};

pub const INIT_PATH: &str = "/v1/vault/init";
/// Packed mouse samples, base64: 2 048 samples of 16 bytes are about 44 KiB. A cap of its own, below the
/// signed-body cap the other routes share.
pub const MAX_VAULT_BODY_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitBody {
    samples: String,
}

pub async fn status(State(ctx): State<ApiContext>) -> Response {
    match ctx.vault.status() {
        Ok(status) => Json(status_json(status)).into_response(),
        Err(refusal) => refusal_error(refusal).into_response(),
    }
}

/// A body that is not exactly `{"samples": "<base64>"}` is refused as a whole. The phrase is the one secret
/// the answer carries: it is `no-store` and never audited.
pub async fn init(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Response {
    let Ok(body) = serde_json::from_slice::<InitBody>(&body) else {
        return ApiError::invalid_body().into_response();
    };
    let Ok(samples) = STANDARD.decode(body.samples) else {
        return ApiError::invalid_body().into_response();
    };
    let vault = Arc::clone(&ctx.vault);
    let phrase = match tokio::task::spawn_blocking(move || vault.init(samples)).await {
        Ok(Ok(phrase)) => phrase,
        Ok(Err(refusal)) => return refusal_error(refusal).into_response(),
        Err(error) => return ApiError::internal(error).into_response(),
    };
    ctx.audit.record(&AuditEvent::new(
        "vault_initialised",
        Some(&verified.device_id),
        "initialised",
    ));
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"ok": true, "phrase": phrase})),
    )
        .into_response()
}

pub async fn finish(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
) -> Response {
    let vault = Arc::clone(&ctx.vault);
    match tokio::task::spawn_blocking(move || vault.finish()).await {
        Ok(Ok(())) => {}
        Ok(Err(refusal)) => return refusal_error(refusal).into_response(),
        Err(error) => return ApiError::internal(error).into_response(),
    }
    ctx.audit.record(&AuditEvent::new(
        "vault_finished",
        Some(&verified.device_id),
        "finished",
    ));
    Json(json!({"ok": true})).into_response()
}

fn status_json(status: VaultStatus) -> Value {
    json!({
        "ok": true,
        "state": if status.protected { "protected" } else { "unprotected" },
        "backup": status.backup,
        "samples_needed": status.samples_needed,
    })
}

fn refusal_error(refusal: VaultRefusal) -> ApiError {
    match refusal {
        VaultRefusal::AlreadyProtected => ApiError::coded(
            StatusCode::CONFLICT,
            "vault_already_protected",
            "the chat database is already protected",
        ),
        VaultRefusal::NotEnoughSamples => ApiError::coded(
            StatusCode::BAD_REQUEST,
            "vault_samples_insufficient",
            "not enough mouse samples to make a key",
        ),
        VaultRefusal::BadSamples => ApiError::coded(
            StatusCode::BAD_REQUEST,
            "vault_samples_invalid",
            "the samples are not packed 16-byte records",
        ),
        VaultRefusal::NoBackup => ApiError::coded(
            StatusCode::CONFLICT,
            "vault_no_backup",
            "there is no plain backup to finish",
        ),
        VaultRefusal::KeyStoreFailed => ApiError::coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "vault_key_failed",
            "the vault key could not be stored; nothing changed",
        ),
        VaultRefusal::MigrationFailed => ApiError::coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "vault_migration_failed",
            "the chat database could not be protected; see the host log",
        ),
        VaultRefusal::Failed => ApiError::coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "vault_failed",
            "the vault could not answer",
        ),
        VaultRefusal::Unavailable => ApiError::coded(
            StatusCode::SERVICE_UNAVAILABLE,
            "vault_unavailable",
            "this host has no vault",
        ),
    }
}
