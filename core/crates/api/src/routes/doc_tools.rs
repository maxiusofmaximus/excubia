//! `GET /v1/doc-tools`: the document CLIs this PC has (DOC7A).
//!
//! Always the three apps, in `App::ALL` order. Names, availability and the
//! tested version only: never a command or a path. The host discovers the CLIs
//! when it starts; every read serves that discovery.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::ApiContext;

pub async fn list(State(ctx): State<ApiContext>) -> Json<Value> {
    let tools: Vec<Value> = ctx
        .docs
        .tools()
        .into_iter()
        .map(|tool| {
            json!({
                "app": tool.app,
                "name": tool.name,
                "available": tool.available,
                "tested_version": tool.tested_version,
            })
        })
        .collect();
    Json(json!({"ok": true, "tools": tools}))
}
