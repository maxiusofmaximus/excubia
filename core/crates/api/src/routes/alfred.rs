//! `GET /v1/alfred/backend` (`READ_STATE`) and `POST /v1/alfred/backend` (`APPROVE`): which engine
//! answers Alfred, and the owner's switch between the two. The switch runs on the host in the
//! background, so the answer never waits for the local model to load.

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::Value;

use crate::alfred::Requested;
use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

#[derive(Deserialize)]
struct BackendBody {
    #[serde(default)]
    backend: Option<Value>,
}

pub async fn status(State(ctx): State<ApiContext>) -> Json<Value> {
    Json(ctx.alfred.status().await.to_json())
}

pub async fn request(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let payload: BackendBody = parse_body(&body)?;
    let backend = match payload.backend.as_ref().and_then(Value::as_str) {
        Some(name @ ("local" | "claude")) => name,
        _ => {
            return Err(ApiError::action(
                "invalid_backend",
                "backend must be local or claude",
            ))
        }
    };
    let requested = ctx
        .alfred
        .request(backend)
        .await
        .map_err(|message| ApiError::coded(StatusCode::CONFLICT, "backend_busy", message))?;
    let (code, outcome) = match requested {
        Requested::Unchanged => (StatusCode::OK, "unchanged"),
        Requested::Started => (StatusCode::ACCEPTED, "started"),
    };
    eprintln!(
        "action alfred.backend device={} backend={backend} outcome={outcome}",
        verified.device_id
    );
    let status = ctx.alfred.status().await.to_json();
    Ok((code, Json(status)).into_response())
}
