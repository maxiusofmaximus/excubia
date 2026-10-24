//! User sections: list, create and rename. "Recientes" is a query, not a row.

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::routes::parse_body;
use crate::ApiContext;

use super::support;

#[derive(Deserialize)]
struct UpsertBody {
    #[serde(default)]
    id: Option<String>,
    name: String,
}

pub async fn list(State(ctx): State<ApiContext>) -> Response {
    match ctx.chat.sections() {
        Ok(sections) => support::map_ok("sections", sections),
        Err(error) => support::map_error(error).into_response(),
    }
}

pub async fn upsert(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: UpsertBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let name = match support::section_name(&payload.name) {
        Ok(name) => name,
        Err(error) => return error.into_response(),
    };
    match ctx.chat.upsert_section(payload.id.as_deref(), &name) {
        Ok(section) => support::map_ok("section", section),
        Err(error) => support::map_section_error(error).into_response(),
    }
}
