//! The mail routes (MAIL1D §1): an admin starts a device login, reads where it
//! stands and lists the saved accounts. The host does the work through
//! [`MailActions`]; this module checks the request and shapes the answer.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::ApiError;
use crate::mail::{MailFailure, MailLoginStart, TENANTS};
use crate::routes::parse_body;
use crate::ApiContext;

const MAX_ACCOUNT_CHARS: usize = 64;

#[derive(Deserialize)]
struct StartBody {
    account: String,
    tenant: String,
    /// The conversation the sign-in card shows in, when there is one.
    #[serde(default)]
    conversation_id: Option<String>,
}

/// The answer to a start: the login fields, flat, next to the `ok` flag.
#[derive(Serialize)]
pub(crate) struct StartReply<'a> {
    pub(crate) ok: bool,
    #[serde(flatten)]
    pub(crate) login: &'a MailLoginStart,
}

/// `POST /v1/mail/login/start` (signed, `ADMIN`): begin a device login.
pub async fn login_start(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: StartBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = check_account(&payload.account, "account") {
        return error.into_response();
    }
    if !TENANTS.contains(&payload.tenant.as_str()) {
        return ApiError::invalid_field(
            "mail_invalid_input",
            "tenant",
            "tenant must be consumers, organizations or common",
        )
        .into_response();
    }
    let conversation = payload.conversation_id.as_deref();
    match ctx
        .mail
        .login_start(&payload.account, &payload.tenant, conversation)
        .await
    {
        Ok(login) => Json(StartReply {
            ok: true,
            login: &login,
        })
        .into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `GET /v1/mail/login/status?account=` (signed, `ADMIN`): where the latest
/// login for that account stands. Its answer is `pending`, `ok`, `denied`,
/// `expired` or `error` with a stable code.
pub async fn login_status(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let account = query.get("account").map(String::as_str).unwrap_or_default();
    if let Err(error) = check_account(account, "account") {
        return error.into_response();
    }
    match ctx.mail.login_status(account) {
        Ok(status) => Json(json!({"ok": true, "login": status})).into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `GET /v1/mail/accounts` (signed, `ADMIN`): the saved accounts, without tokens.
pub async fn accounts(State(ctx): State<ApiContext>) -> Response {
    match ctx.mail.accounts() {
        Ok(accounts) => Json(json!({"ok": true, "accounts": accounts})).into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// A label the user chose: printable, non-empty, short.
pub(crate) fn check_account(account: &str, field: &str) -> Result<(), ApiError> {
    let trimmed = account.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_ACCOUNT_CHARS
        || trimmed.chars().any(char::is_control)
    {
        return Err(ApiError::invalid_field(
            "mail_invalid_input",
            field,
            "account must be a label of 1 to 64 printable characters",
        ));
    }
    Ok(())
}

/// A refusal with its stable code. A rate limit is the one 429, with `Retry-After`.
pub(crate) fn failure_response(failure: MailFailure) -> Response {
    let mut error = ApiError::action(failure.code, failure.message);
    if let Some(retry_after) = failure.retry_after {
        error.status = StatusCode::TOO_MANY_REQUESTS;
        error.retry_after = Some(retry_after);
    }
    error.into_response()
}
