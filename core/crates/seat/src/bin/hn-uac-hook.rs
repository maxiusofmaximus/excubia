//! `hn-uac-hook`: the blocking hook of the CLIs that support one.
//!
//! A CLI with a pre-tool hook runs this binary before the risky action. It
//! opens an approval in the host, waits for the operator and answers with the
//! exit code the CLI expects:
//!
//! - `0` — the action may run: the operator allowed it and the approval was
//!   consumed just before.
//! - `2` — the action is denied. Covers the operator's refusal, the expired
//!   approval, every way the host can fail to answer (timeout, refused
//!   connection, malformed response) **and** a wrong invocation of the binary.
//!   Fail closed: no answer is no permission, and a CLI must never read the
//!   hook as "broken" and run the action.
//!
//! The credentials come from the environment (`HN_HOOK_URL`, `HN_HOOK_TOKEN`),
//! never from the arguments. The token is the one minted for this run.

use std::io::Read;
use std::panic::{self, UnwindSafe};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use hn_core_chat::action_digest;
use hn_core_seat::hook_action;
use hn_core_seat::http::{self, LoopbackUrl};
use serde_json::{json, Value};

/// The action may run.
const EXIT_ALLOWED: u8 = 0;
/// The action is denied, or the host never gave an answer.
const EXIT_DENIED: u8 = 2;
/// How often the hook asks the host for the state.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// One request to the host may not hang the hook for longer than this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

fn main() -> ExitCode {
    ExitCode::from(guarded(|| exit_code(execute())))
}

/// Runs the hook and turns a panic into a denial. Claude Code blocks only on
/// `2`: a panic that left with `101` would be a non-blocking error, and the
/// action would run.
fn guarded(hook: impl FnOnce() -> u8 + UnwindSafe) -> u8 {
    panic::catch_unwind(hook).unwrap_or(EXIT_DENIED)
}

fn exit_code(result: Result<Outcome, Refusal>) -> u8 {
    match result {
        Ok(Outcome::Allowed) => EXIT_ALLOWED,
        Ok(Outcome::Denied(reason)) | Err(Refusal::NoDecision(reason) | Refusal::Usage(reason)) => {
            eprintln!("hn-uac-hook: {reason}");
            EXIT_DENIED
        }
    }
}

fn execute() -> Result<Outcome, Refusal> {
    let request = Request::read()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Refusal::NoDecision(error.to_string()))?;
    runtime.block_on(run(&request))
}

/// Why there is no permission. `Usage` is not a decision, but it still exits
/// with `2`: a CLI that saw a different code could treat the hook as broken
/// and run the action.
#[derive(Debug)]
enum Refusal {
    /// The binary was called wrong: an invalid argument. Exit 2.
    Usage(String),
    /// No decision: denied, expired or the host did not answer. Exit 2.
    NoDecision(String),
}

/// What the agent wants to do.
struct Request {
    kind: String,
    tool: String,
    summary: String,
    args: Value,
    cwd: String,
    paths: Vec<String>,
    ttl_seconds: u64,
}

impl Request {
    /// The arguments win over the payload of the CLI: an explicit flag is
    /// what the operator configured, the payload is what the CLI guessed.
    fn read() -> Result<Self, Refusal> {
        let flags = Flags::parse(std::env::args().skip(1))?;
        let payload = if flags.tool.is_none() {
            read_payload()?
        } else {
            None
        };
        let tool = flags
            .tool
            .or_else(|| payload.as_ref().and_then(|p| p.tool_name.clone()))
            .ok_or_else(|| {
                Refusal::Usage("the hook needs a tool (--tool or a payload on stdin)".to_string())
            })?;
        let args = flags
            .args
            .or_else(|| payload.as_ref().and_then(|p| p.args.clone()))
            .unwrap_or_else(|| json!({}));
        // The tool table says what the call is (an edit names its file); a tool
        // it does not know keeps the old default, a command.
        let mapped = hook_action::of(&tool, &args);
        let kind = flags
            .kind
            .or_else(|| payload.as_ref().and_then(|p| p.kind.clone()))
            .or_else(|| mapped.as_ref().map(|action| action.kind.to_string()))
            .unwrap_or_else(|| "command".to_string());
        let summary = flags
            .summary
            .or_else(|| payload.as_ref().and_then(|p| p.summary.clone()))
            .unwrap_or_else(|| format!("{kind} {tool}"));
        let cwd = flags
            .cwd
            .or_else(|| payload.as_ref().and_then(|p| p.cwd.clone()))
            .unwrap_or_default();
        let paths = match (flags.paths.is_empty(), mapped) {
            (true, Some(action)) => action.paths,
            _ => flags.paths,
        };
        Ok(Self {
            kind,
            tool,
            summary,
            args,
            cwd,
            paths,
            ttl_seconds: flags.ttl_seconds,
        })
    }
}

/// The command line. Every problem here is a usage error: the hook never
/// guesses what the operator meant.
#[derive(Debug)]
struct Flags {
    kind: Option<String>,
    tool: Option<String>,
    summary: Option<String>,
    args: Option<Value>,
    cwd: Option<String>,
    paths: Vec<String>,
    ttl_seconds: u64,
}

impl Flags {
    fn parse(argv: impl Iterator<Item = String>) -> Result<Self, Refusal> {
        let argv: Vec<String> = argv.collect();
        let mut flags = Self {
            kind: None,
            tool: None,
            summary: None,
            args: None,
            cwd: None,
            paths: Vec::new(),
            // The longest the hook waits when the payload does not say.
            ttl_seconds: u64::from(hn_core_chat::DEFAULT_TTL_SECONDS),
        };
        let mut index = 0;
        while index < argv.len() {
            let argument = argv[index].clone();
            index += 1;
            let mut value = || -> Result<String, Refusal> {
                let value = argv
                    .get(index)
                    .cloned()
                    .ok_or_else(|| Refusal::Usage(format!("{argument} needs a value")))?;
                index += 1;
                Ok(value)
            };
            match argument.as_str() {
                "--kind" => flags.kind = Some(value()?),
                "--tool" => flags.tool = Some(value()?),
                "--summary" => flags.summary = Some(value()?),
                "--args" => {
                    let raw = value()?;
                    flags.args =
                        Some(serde_json::from_str(&raw).map_err(|error| {
                            Refusal::Usage(format!("--args is not JSON: {error}"))
                        })?);
                }
                "--cwd" => flags.cwd = Some(value()?),
                "--paths" => {
                    flags.paths = value()?
                        .split(',')
                        .map(|path| path.trim().to_string())
                        .filter(|path| !path.is_empty())
                        .collect();
                }
                "--ttl" => {
                    let raw = value()?;
                    flags.ttl_seconds = raw.parse().map_err(|_| {
                        Refusal::Usage("--ttl needs a number of seconds".to_string())
                    })?;
                }
                other => {
                    return Err(Refusal::Usage(format!("unknown argument: {other}")));
                }
            }
        }
        Ok(flags)
    }
}

/// The payload a CLI hands to its hook on stdin.
struct Payload {
    tool_name: Option<String>,
    kind: Option<String>,
    summary: Option<String>,
    args: Option<Value>,
    cwd: Option<String>,
}

fn read_payload() -> Result<Option<Payload>, Refusal> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|error| Refusal::NoDecision(format!("cannot read the hook payload: {error}")))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| Refusal::NoDecision(format!("the hook payload is not JSON: {error}")))?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.to_string())
    };
    Ok(Some(Payload {
        tool_name: text("tool_name").or_else(|| text("tool")),
        kind: text("kind"),
        summary: text("summary"),
        args: value
            .get("tool_input")
            .cloned()
            .or_else(|| value.get("args").cloned()),
        cwd: text("cwd"),
    }))
}

/// What the host answered.
enum Outcome {
    Allowed,
    Denied(String),
}

/// What the hook learns from one poll.
enum State {
    Pending,
    Approved,
    Refused(String),
}

async fn run(request: &Request) -> Result<Outcome, Refusal> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Flags, Refusal> {
        Flags::parse(arguments.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn an_unknown_argument_is_a_usage_error() {
        let error = parse(&["--tool", "bash", "--nope"]).expect_err("unknown flag");
        assert!(matches!(error, Refusal::Usage(_)));
    }

    #[test]
    fn a_missing_value_is_a_usage_error() {
        let error = parse(&["--tool"]).expect_err("missing value");
        assert!(matches!(error, Refusal::Usage(_)));
    }

    #[test]
    fn a_panic_is_a_denial_not_an_unblocking_exit() {
        assert_eq!(guarded(|| panic!("boom")), EXIT_DENIED);
    }

    #[test]
    fn a_normal_answer_passes_through_the_guard() {
        assert_eq!(guarded(|| EXIT_ALLOWED), EXIT_ALLOWED);
        assert_eq!(guarded(|| EXIT_DENIED), EXIT_DENIED);
    }

    #[test]
    fn the_arguments_become_a_request() {
        let flags = parse(&[
            "--kind", "command", "--tool", "bash", "--ttl", "5", "--paths", "a, b",
        ])
        .expect("flags");
        assert_eq!(flags.kind.as_deref(), Some("command"));
        assert_eq!(flags.tool.as_deref(), Some("bash"));
        assert_eq!(flags.ttl_seconds, 5);
        assert_eq!(flags.paths, vec!["a".to_string(), "b".to_string()]);
    }
}
