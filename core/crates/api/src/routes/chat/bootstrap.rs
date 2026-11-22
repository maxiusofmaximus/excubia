//! `GET /v1/chat/bootstrap`: the one-call home screen.

use axum::extract::State;
use axum::response::{IntoResponse, Response};

use crate::ApiContext;

use super::support;

pub async fn bootstrap(State(ctx): State<ApiContext>) -> Response {
    match ctx.chat.bootstrap() {
        Ok(bootstrap) => support::ok_object(bootstrap),
        Err(error) => support::map_error(error).into_response(),
    }
}
