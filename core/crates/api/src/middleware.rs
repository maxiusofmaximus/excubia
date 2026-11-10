//! The signed-request middleware: authentication, authorization and the rate
//! budgets, in the contract's order (`docs/contracts/errors.md`).
//!
//! The body is buffered once, here, because the signature covers the exact
//! received bytes: re-reading it downstream would be a second view of the
//! request.

use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use excubia_core_auth::{
    authorize, classify, Budget, Class, HeaderSource, SignedRequest, VerifiedDevice,
    MAX_SIGNED_BODY_BYTES,
};
use excubia_core_chat::{HookScope, HookTokenUse};

use crate::error::ApiError;
use crate::routes::{IMPORT_PATH, MAX_ICS_BODY_BYTES};
use crate::voice::{self, MAX_VOICE_BODY_BYTES};
use crate::{ApiContext, PeerAddr};

/// The header a hook presents its host token in.
pub const HEADER_HOOK_TOKEN: &str = "X-Excubia-Hook-Token";
/// legacy: the name hooks installed before the rename send; read when the
/// current one is absent, never emitted.
const LEGACY_HEADER_HOOK_TOKEN: &str = "X-HN-Hook-Token";

/// What a hook request proved, and what it may touch.
#[derive(Clone, Debug)]
pub enum HookAuth {
    /// The host-wide token: pane reports only. It names no run, so it can
    /// never open, read or burn an approval.
    Host,
    /// A live token of one run or one resident panel.
    Scoped(HookScope),
}

struct Headers<'a>(&'a HeaderMap);

impl HeaderSource for Headers<'_> {
    fn header(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(|value| value.to_str().ok())
    }
}

pub async fn authenticate(State(ctx): State<ApiContext>, request: Request, next: Next) -> Response {
    match authenticate_inner(&ctx, request).await {
        Ok(request) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}

async fn authenticate_inner(ctx: &ApiContext, request: Request) -> Result<Request, ApiError> {
    let (mut parts, body) = request.into_parts();
    let target = parts
        .uri
        .path_and_query()
        .map(|path| path.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let bytes = to_bytes(body, body_limit(&parts.method, parts.uri.path()))
        .await
        .map_err(|_| ApiError::body_too_large())?;

    // The trust store can be rewritten by the CLI while this server runs.
    ctx.devices.reload_if_changed();

    let headers = Headers(&parts.headers);
    let verified = ctx
        .verifier
        .verify(&SignedRequest {
            method: parts.method.as_str(),
            target: &target,
            body: &bytes,
            headers: &headers,
        })
        .map_err(|error| ApiError::unauthorized(error.reason.label()))?;

    let rule = classify(parts.method.as_str(), &target)
        .map_err(|error| ApiError::forbidden(error.reason, error.required, &verified.device_id))?;
    authorize(&verified.permissions, &rule)
        .map_err(|error| ApiError::forbidden(error.reason, error.required, &verified.device_id))?;

    consume_budget(ctx, &verified, rule.budget)?;
    if let Err(error) = ctx.devices.touch(&verified.device_id) {
        eprintln!("auth: could not touch {}: {error}", verified.device_id);
    }

    parts.extensions.insert(verified);
    Ok(Request::from_parts(parts, Body::from(bytes)))
}

/// The cap a signed body is read under. The voice note is the one route with a larger one.
fn body_limit(method: &Method, path: &str) -> usize {
    if *method == Method::POST && path == voice::TRANSCRIBE_PATH {
        MAX_VOICE_BODY_BYTES
    } else if *method == Method::POST && path == IMPORT_PATH {
        MAX_ICS_BODY_BYTES
    } else {
        MAX_SIGNED_BODY_BYTES
    }
}

fn consume_budget(
    ctx: &ApiContext,
    verified: &VerifiedDevice,
    budget: Budget,
) -> Result<(), ApiError> {
    ctx.limiter
        .allow(&verified.device_id, Class::Read)
        .map_err(ApiError::slow_down)?;
    if budget == Budget::Write {
        ctx.limiter
            .allow(&verified.device_id, Class::Write)
            .map_err(ApiError::slow_down)?;
    }
    Ok(())
}

/// The hook guard: hooks are local processes, so they must come from loopback
/// and present a credential. Two credentials exist and they are not
/// interchangeable: the host token reports pane state, and a per-run (or
/// per-panel) token opens, reads and burns approvals. A device signature is
/// never accepted here, and neither token is accepted on the device routes.
pub async fn hook_guard(
    State(ctx): State<ApiContext>,
    ConnectInfo(PeerAddr { addr, .. }): ConnectInfo<PeerAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    if !addr.ip().is_loopback() {
        return ApiError::forbidden(
            excubia_core_auth::AuthzReason::NotPermitted,
            None,
            "hook (non-loopback)",
        )
        .into_response();
    }
    let provided = presented_hook_token(request.headers());
    let auth = match ctx.chat.hook_token_use(provided) {
        Ok(HookTokenUse::Live(scope)) => HookAuth::Scoped(scope),
        Ok(HookTokenUse::Revoked) => {
            return ApiError::forbidden_code("hook_token_revoked").into_response()
        }
        Ok(HookTokenUse::Unknown) => {
            if !bool::from(provided.as_bytes().ct_eq(ctx.hook_token.as_bytes())) {
                return ApiError::unauthorized("hook token").into_response();
            }
            HookAuth::Host
        }
        Err(error) => return ApiError::internal(error).into_response(),
    };
    request.extensions_mut().insert(auth);
    next.run(request).await
}

/// The token a hook presents. The pre-rename header is read only when the current one is absent.
fn presented_hook_token(headers: &HeaderMap) -> &str {
    headers
        .get(HEADER_HOOK_TOKEN)
        .or_else(|| headers.get(LEGACY_HEADER_HOOK_TOKEN))
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn a_hook_presenting_the_legacy_header_is_read_and_the_current_one_wins() {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_HEADER_HOOK_TOKEN, HeaderValue::from_static("old"));
        assert_eq!(presented_hook_token(&headers), "old");
        headers.insert(HEADER_HOOK_TOKEN, HeaderValue::from_static("new"));
        assert_eq!(presented_hook_token(&headers), "new");
    }
}
