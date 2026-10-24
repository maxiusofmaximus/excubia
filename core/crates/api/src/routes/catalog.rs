//! `GET /v1/catalog`: the agents this host is willing to start, by name.
//!
//! Names and availability only: a client can pick an entry, never learn a
//! command, its arguments or a resolved path. The catalogue is re-read when
//! `agents.toml` changes; while the file is broken the last good copy is
//! served and `warning` says why.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::ApiContext;

pub async fn list(State(ctx): State<ApiContext>) -> Json<Value> {
    let agents: Vec<Value> = ctx
        .agents
        .entries()
        .into_iter()
        .map(|entry| {
            let mut value = json!({"name": entry.name, "available": entry.available});
            if let Some(label) = entry.label {
                value["label"] = json!(label);
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
