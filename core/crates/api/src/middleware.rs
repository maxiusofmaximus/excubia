//! The signed-request middleware: authentication, authorization and the rate
//! budgets, in the contract's order (`docs/contracts/errors.md`).
//!
//! The body is buffered once, here, because the signature covers the exact
//! received bytes: re-reading it downstream would be a second view of the
//! request.

use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use hn_core_auth::{
    authorize, classify, Budget, Class, HeaderSource, SignedRequest, VerifiedDevice,
    MAX_SIGNED_BODY_BYTES,
};
use hn_core_chat::{HookScope, HookTokenUse};

use crate::error::ApiError;
use crate::{ApiContext, PeerAddr};

/// The header a hook presents its host token in.
pub const HEADER_HOOK_TOKEN: &str = "X-HN-Hook-Token";

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
    let bytes = to_bytes(body, MAX_SIGNED_BODY_BYTES)
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
    ConnectInfo(PeerAddr(addr)): ConnectInfo<PeerAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    if !addr.ip().is_loopback() {
        return ApiError::forbidden(
            hn_core_auth::AuthzReason::NotPermitted,
            None,
            "hook (non-loopback)",
        )
        .into_response();
    }
    let provided = request
        .headers()
        .get(HEADER_HOOK_TOKEN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
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
