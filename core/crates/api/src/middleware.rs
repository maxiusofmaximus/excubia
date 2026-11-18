//! The signed-request middleware: authentication, authorization and the rate
//! budgets, in the contract's order (`docs/contracts/errors.md`).
//!
//! The body is buffered once, here, because the signature covers the exact
//! received bytes: re-reading it downstream would be a second view of the
//! request.

use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::{CONNECTION, CONTENT_LENGTH};
use axum::http::{HeaderMap, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use futures_core::Stream;
use subtle::ConstantTimeEq;

use excubia_auth::{
    authorize, classify, Budget, Class, HeaderSource, SignedRequest, VerifiedDevice,
    MAX_SIGNED_BODY_BYTES,
};
use excubia_chat::{HookScope, HookTokenUse};

use crate::error::ApiError;
use crate::routes::{IMPORT_PATH, MAX_ICS_BODY_BYTES, MAX_VAULT_BODY_BYTES, VAULT_INIT_PATH};
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
        Err(error) => {
            // A refused request may leave its body unread, and nothing then
            // bounds the wait for it: the connection closes after the answer.
            let mut response = error.into_response();
            response
                .headers_mut()
                .insert(CONNECTION, HeaderValue::from_static("close"));
            response
        }
    }
}

async fn authenticate_inner(ctx: &ApiContext, request: Request) -> Result<Request, ApiError> {
    let (mut parts, body) = request.into_parts();
    let target = parts
        .uri
        .path_and_query()
        .map(|path| path.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let (limit, timeout) = body_policy(&parts.method, parts.uri.path());

    // The trust store can be rewritten by the CLI while this server runs.
    ctx.devices.reload_if_changed();

    // Nothing is buffered for a request that cannot be trusted. The body-free
    // checks run first (`401` without reading a byte); then a declared size
    // over the cap is drained, not buffered, so the `413` reaches a client that
    // is still sending; only a body within the cap is read.
    ctx.verifier
        .precheck(&Headers(&parts.headers))
        .map_err(|error| ApiError::unauthorized(error.reason.label()))?;
    if declared_length(&parts.headers).is_some_and(|length| length > limit) {
        discard(body, timeout).await;
        return Err(ApiError::body_too_large());
    }
    let bytes = read_body(body, limit, timeout).await?;

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

/// How long a signed body may take to arrive. A body that stalls past it is
/// answered `408` and never reaches the signature. The routes with a larger
/// cap (voice, .ics, vault) get the longer time.
const BODY_READ_TIMEOUT: Duration = Duration::from_secs(10);
const LONG_BODY_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The cap and the read time a signed body is held to.
fn body_policy(method: &Method, path: &str) -> (usize, Duration) {
    if *method == Method::POST && path == voice::TRANSCRIBE_PATH {
        (MAX_VOICE_BODY_BYTES, LONG_BODY_READ_TIMEOUT)
    } else if *method == Method::POST && path == VAULT_INIT_PATH {
        (MAX_VAULT_BODY_BYTES, LONG_BODY_READ_TIMEOUT)
    } else if *method == Method::POST && path == IMPORT_PATH {
        (MAX_ICS_BODY_BYTES, LONG_BODY_READ_TIMEOUT)
    } else {
        (MAX_SIGNED_BODY_BYTES, BODY_READ_TIMEOUT)
    }
}

/// The length the client declared, when it declared one that parses.
fn declared_length(headers: &HeaderMap) -> Option<usize> {
    headers.get(CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

/// Buffers a body under its cap. Over the cap is `413`; too slow is `408`.
async fn read_body(body: Body, limit: usize, timeout: Duration) -> Result<Bytes, ApiError> {
    match tokio::time::timeout(timeout, to_bytes(body, limit)).await {
        Ok(read) => read.map_err(|_| ApiError::body_too_large()),
        Err(_) => Err(ApiError::request_timeout()),
    }
}

/// Reads a body to nothing, under its time limit, without buffering it. A
/// server that closes on unread input resets the connection, and the client
/// then never sees the answer; this lets an over-cap body reach its `413`.
async fn discard(body: Body, timeout: Duration) {
    let mut chunks = body.into_data_stream();
    let drain = std::future::poll_fn(|cx| loop {
        match Pin::new(&mut chunks).poll_next(cx) {
            Poll::Ready(Some(Ok(_))) => {}
            Poll::Ready(Some(Err(_)) | None) => return Poll::Ready(()),
            Poll::Pending => return Poll::Pending,
        }
    });
    let _ = tokio::time::timeout(timeout, drain).await;
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
            excubia_auth::AuthzReason::NotPermitted,
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

    #[test]
    fn a_hook_presenting_the_legacy_header_is_read_and_the_current_one_wins() {
        let mut headers = HeaderMap::new();
        headers.insert(LEGACY_HEADER_HOOK_TOKEN, HeaderValue::from_static("old"));
        assert_eq!(presented_hook_token(&headers), "old");
        headers.insert(HEADER_HOOK_TOKEN, HeaderValue::from_static("new"));
        assert_eq!(presented_hook_token(&headers), "new");
    }

    /// A body whose bytes never come: the stall a slow or hostile client makes.
    struct Stalled;

    impl futures_core::Stream for Stalled {
        type Item = Result<Bytes, std::io::Error>;

        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Pending
        }
    }

    #[tokio::test]
    async fn a_body_that_never_arrives_is_408_at_its_read_time() {
        let started = std::time::Instant::now();
        let error = read_body(Body::from_stream(Stalled), 1024, Duration::from_millis(50))
            .await
            .expect_err("stalled");
        assert_eq!(error.status, axum::http::StatusCode::REQUEST_TIMEOUT);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_body_over_its_cap_is_413_and_one_at_the_cap_is_read_whole() {
        let over = read_body(Body::from("abcde"), 4, Duration::from_secs(5))
            .await
            .expect_err("over the cap");
        assert_eq!(over.status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
        let whole = read_body(Body::from("abcd"), 4, Duration::from_secs(5))
            .await
            .expect("at the cap");
        assert_eq!(&whole[..], b"abcd");
    }

    #[test]
    fn a_declared_length_is_read_only_when_it_parses() {
        let mut headers = HeaderMap::new();
        assert_eq!(declared_length(&headers), None);
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("300000"));
        assert_eq!(declared_length(&headers), Some(300_000));
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("many"));
        assert_eq!(declared_length(&headers), None);
    }
}
