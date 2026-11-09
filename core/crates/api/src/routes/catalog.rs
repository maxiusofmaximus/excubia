//! `GET /v1/catalog`: the agents this host is willing to start, by name.
//!
//! Names and availability only: a client can pick an entry, never learn a
//! command, its arguments or a resolved path. The catalogue is re-read when
//! `agents.toml` changes; while the file is broken the last good copy is
//! served and `warning` says why.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hn_core_auth::VerifiedDevice;
use serde_json::{json, Value};

use crate::audit::AuditEvent;
use crate::catalog::{FixError, FixOutcome};
use crate::error::ApiError;
use crate::ApiContext;

pub async fn list(State(ctx): State<ApiContext>) -> Json<Value> {
    let agents: Vec<Value> = ctx
        .agents
        .entries()
        .into_iter()
        .map(|entry| {
            let mut value = json!({
                "name": entry.name,
                "available": entry.available,
                "runnable": entry.runnable,
                "fixable": entry.fixable,
            });
            if let Some(label) = entry.label {
                value["label"] = json!(label);
            }
            if !entry.runnable {
                value["problem"] = json!(if entry.fixable {
                    "Windows no puede ejecutarlo tal como está instalado; se puede arreglar."
                } else if entry.available {
                    "Windows no puede ejecutarlo tal como está instalado."
                } else {
                    "No está instalado en este equipo."
                });
            }
            value
        })
        .collect();
    let mut body = json!({"ok": true, "agents": agents});
    if let Some(warning) = ctx.agents.warning() {
        body["warning"] = json!(warning);
    }
    Json(body)
}

/// `POST /v1/catalog/{name}/fix` (APPROVE): point an entry that Windows cannot start at its `.CMD`
/// shim. The paths stay on the host; the answer and the audit name only the agent.
pub async fn fix(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let outcome = ctx.agents.fix(&name);
    let label = match &outcome {
        Ok(FixOutcome::AlreadyRunnable) => "unchanged",
        Ok(FixOutcome::Fixed) => "fixed",
        Err(FixError::UnknownAgent) => "unknown",
        Err(FixError::NotFixable) => "not_fixable",
        Err(FixError::Write(_)) => "failed",
    };
    ctx.audit.record(
        &AuditEvent::new("catalog.fix", Some(&verified.device_id), label)
            .with("agent", name.as_str()),
    );
    match outcome {
        Ok(FixOutcome::AlreadyRunnable) => {
            Ok(Json(json!({"ok": true, "fixed": false})).into_response())
        }
        Ok(FixOutcome::Fixed) => {
            Ok(Json(json!({"ok": true, "fixed": true, "name": name})).into_response())
        }
        Err(FixError::UnknownAgent) => Err(ApiError::coded(
            StatusCode::NOT_FOUND,
            "unknown_agent",
            "no hay ningún agente con ese nombre",
        )),
        Err(FixError::NotFixable) => Err(ApiError::coded(
            StatusCode::CONFLICT,
            "not_fixable",
            "este agente no tiene nada que arreglar aquí",
        )),
        Err(FixError::Write(reason)) => {
            eprintln!("host: catalog fix agent={name} failed: {reason}");
            Err(ApiError::coded(
                StatusCode::INTERNAL_SERVER_ERROR,
                "write_failed",
                "no pude cambiar agents.toml; no cambié nada",
            ))
        }
    }
}
