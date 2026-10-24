//! `POST /v1/hooks/report`: what an agent's own hook tells the host.
//!
//! Not a device route: hooks are local processes, so they authenticate with
//! the host token (loopback only), never with a device signature. The report
//! wins over screen patterns while it is fresh (`hooks.rs` in the host owns
//! the freshness rule).

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::validate::validate_pane_id;
use crate::{ApiContext, HookState};

#[derive(Deserialize)]
struct HookReport {
    pane_id: String,
    state: String,
    #[serde(default)]
    message: Option<String>,
}

pub async fn report(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: HookReport = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = validate_pane_id(&payload.pane_id) {
        return error.into_response();
    }
    let Some(state) = HookState::from_label(&payload.state) else {
        return ApiError::invalid_body_field(
            "state",
            format!(
                "unknown state {:?}; expected idle|working|blocked|unknown",
                payload.state
            ),
        )
        .into_response();
    };
    let Some(row) = ctx.store.row_of(&payload.pane_id).await else {
        return ApiError::pane_gone(&payload.pane_id).into_response();
    };
    if row.restored {
        return ApiError::refused("the pane has no running agent").into_response();
    }
    match ctx
        .actions
        .report_agent(&payload.pane_id, state, payload.message.as_deref())
        .await
    {
        Ok(status) => Json(json!({
            "ok": true,
            "pane_id": payload.pane_id,
            "agent_status": status,
        }))
        .into_response(),
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}
