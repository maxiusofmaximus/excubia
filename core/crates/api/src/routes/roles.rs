//! `GET /v1/roles`: the role catalogue, the professions a person can hire.
//!
//! Read-only and the same for every device: nothing in it comes from the
//! host's state. It is not `GET /v1/catalog`, which lists the CLIs this host
//! can start.

use axum::Json;
use serde_json::{json, Value};

use crate::role_catalog::RoleCatalog;
use crate::ApiError;

pub async fn list() -> Result<Json<Value>, ApiError> {
    let catalog = RoleCatalog::embedded().map_err(ApiError::internal)?;
    Ok(Json(json!({
        "ok": true,
        "version": catalog.version,
        "areas": catalog.areas,
        "roles": catalog.roles,
    })))
}
