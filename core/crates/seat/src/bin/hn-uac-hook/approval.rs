//! The talk with the host: open an approval, wait for the operator's answer and
//! consume the approval just before the action runs.

use std::time::{Duration, Instant};

use hn_core_chat::action_digest;
use hn_core_seat::http::{self, LoopbackUrl};
use serde_json::{json, Value};

use super::request::Request;
use super::Refusal;

/// How often the hook asks the host for the state.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// One request to the host may not hang the hook for longer than this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What the host answered.
pub(super) enum Outcome {
    Allowed,
    Denied(String),
}

/// What the hook learns from one poll.
enum State {
    Pending,
    Approved,
    Refused(String),
}

pub(super) async fn run(request: &Request) -> Result<Outcome, Refusal> {
    let base = std::env::var("HN_HOOK_URL")
        .map_err(|_| Refusal::NoDecision("HN_HOOK_URL is not set".to_string()))?;
    let token = std::env::var("HN_HOOK_TOKEN")
        .map_err(|_| Refusal::NoDecision("HN_HOOK_TOKEN is not set".to_string()))?;
    let host = LoopbackUrl::parse(&base).map_err(Refusal::NoDecision)?;
    let digest = action_digest(
        &request.kind,
        &request.tool,
        &request.args,
        &request.cwd,
        &request.paths,
    );
    let body = json!({
        "kind": request.kind,
        "tool": request.tool,
        "summary": request.summary,
        "action_digest": digest,
        "cwd": request.cwd,
        "paths": request.paths,
        "payload_redacted": request.args,
        "ttl_seconds": request.ttl_seconds,
    });
    let created = http::request(
        &host.at("/v1/chat/approvals"),
        "POST",
        &[
            ("Content-Type", "application/json"),
            ("X-HN-Hook-Token", &token),
        ],
        &serde_json::to_vec(&body).map_err(|error| Refusal::NoDecision(error.to_string()))?,
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| {
        Refusal::NoDecision(format!("the host of approvals does not answer: {error}"))
    })?;
    if !(200..300).contains(&created.status) {
        return Err(Refusal::NoDecision(format!(
            "the host of approvals refused the request: HTTP {}",
            created.status
        )));
    }
    let answer = created.json().map_err(|error| {
        Refusal::NoDecision(format!("unreadable answer from the host: {error}"))
    })?;
    let approval_id = answer
        .get("approval_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Refusal::NoDecision("the host did not return an approval id".to_string()))?
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(request.ttl_seconds);

    loop {
        match poll(&host, &token, &approval_id).await? {
            State::Approved => {
                consume(&host, &token, &approval_id, &digest).await?;
                return Ok(Outcome::Allowed);
            }
            State::Refused(reason) => return Ok(Outcome::Denied(reason)),
            State::Pending => {}
        }
        if Instant::now() >= deadline {
            return Ok(Outcome::Denied(
                "the approval expired without an answer".to_string(),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// One look at the approval. The poll is bounded by the TTL, and the hook is a
/// separate process: there is no stream to subscribe to by design.
async fn poll(host: &LoopbackUrl, token: &str, approval_id: &str) -> Result<State, Refusal> {
    let answer = http::request(
        &host.at(&format!("/v1/chat/approvals/{approval_id}")),
        "GET",
        &[("X-HN-Hook-Token", token)],
        &[],
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| {
        Refusal::NoDecision(format!("the host of approvals does not answer: {error}"))
    })?;
    if !(200..300).contains(&answer.status) {
        return Err(Refusal::NoDecision(format!(
            "the host of approvals answered HTTP {}",
            answer.status
        )));
    }
    let state = answer.json().map_err(|error| {
        Refusal::NoDecision(format!("unreadable answer from the host: {error}"))
    })?;
    match state.get("status").and_then(Value::as_str) {
        Some("pending") => Ok(State::Pending),
        Some("approved") => Ok(State::Approved),
        Some("denied") => Ok(State::Refused("the operator denied the action".to_string())),
        Some("expired") => Ok(State::Refused(
            "the approval expired without an answer".to_string(),
        )),
        Some("consumed") => Ok(State::Refused(
            "the approval was already consumed".to_string(),
        )),
        other => Err(Refusal::NoDecision(format!(
            "the host answered an unknown state: {}",
            other.unwrap_or("(none)")
        ))),
    }
}

/// Burn the approval just before the action runs. A failure here denies the
/// action: the approval is single use and the host is the authority.
async fn consume(
    host: &LoopbackUrl,
    token: &str,
    approval_id: &str,
    digest: &str,
) -> Result<(), Refusal> {
    let body = json!({ "action_digest": digest });
    let answer = http::request(
        &host.at(&format!("/v1/chat/approvals/{approval_id}/consume")),
        "POST",
        &[
            ("Content-Type", "application/json"),
            ("X-HN-Hook-Token", token),
        ],
        &serde_json::to_vec(&body).map_err(|error| Refusal::NoDecision(error.to_string()))?,
        REQUEST_TIMEOUT,
    )
    .await
    .map_err(|error| Refusal::NoDecision(format!("the approval could not be consumed: {error}")))?;
    if !(200..300).contains(&answer.status) {
        return Err(Refusal::NoDecision(format!(
            "the approval could not be consumed (HTTP {})",
            answer.status
        )));
    }
    Ok(())
}
