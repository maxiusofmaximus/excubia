//! The exit codes of `hn-uac-hook`: what a CLI learns when the hook answers.
//!
//! `0` is the action allowed and `2` is the action denied: every failure of the
//! host — a timeout, a refused connection, a malformed answer — and a bad
//! invocation of the binary are denials. A CLI must never read the hook as
//! "broken" and run the action. The stubs below stand in for the host; none of
//! these tests spends model quota.

use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// What the stub host does.
#[derive(Clone, Copy)]
enum Behaviour {
    /// It answers every poll with `pending`: nobody ever decides.
    Pending,
    /// It opens the approval and then goes away: the next poll is refused.
    DieAfterCreate,
    /// It answers the create with a body that has no approval id.
    Malformed,
}

struct Stub {
    url: String,
    task: JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Stub {
    async fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
        let address = listener.local_addr().expect("address");
        let task = tokio::spawn(serve(listener, behaviour));
        Self {
            url: format!("http://{address}"),
            task,
        }
    }
}

/// Reads one whole request: the head, then as many body bytes as its
/// `Content-Length` announces. A single read can stop short of the body, and
/// closing the socket with those bytes unread makes Windows reset the reply.
async fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buffer[..end]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if buffer.len() >= end + 4 + length {
            break;
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

async fn serve(listener: TcpListener, behaviour: Behaviour) {
    while let Ok((mut stream, _)) = listener.accept().await {
        let request = read_request(&mut stream).await;
        let creates = request.starts_with("POST /v1/chat/approvals ");
        let approval = r#"{"ok":true,"approval_id":"apr_test","status":"pending","expires_at":"2099-01-01T00:00:00.000Z"}"#;
        let (status, body) = match (behaviour, creates) {
            (Behaviour::Malformed, true) => ("201 Created", "{}"),
            (_, true) => ("201 Created", approval),
            _ => ("200 OK", approval),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        if matches!(behaviour, Behaviour::DieAfterCreate) && creates {
            return;
        }
    }
}

fn hook() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hn-uac-hook"))
}

fn arguments(ttl: &str) -> Vec<String> {
    [
        "--kind",
        "command",
        "--tool",
        "bash",
        "--summary",
        "Ejecutar comando: cargo test",
        "--args",
        r#"{"command":"cargo test"}"#,
        "--cwd",
        ".",
        "--ttl",
        ttl,
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect()
}

async fn run(args: &[String], url: Option<&str>, token: Option<&str>) -> Output {
    let mut command = tokio::process::Command::new(hook());
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
        .env_remove("HN_HOOK_URL")
        .env_remove("HN_HOOK_TOKEN");
    if let Some(url) = url {
        command.env("HN_HOOK_URL", url);
    }
    if let Some(token) = token {
        command.env("HN_HOOK_TOKEN", token);
    }
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("the hook did not finish")
        .expect("run the hook")
}

fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[tokio::test]
async fn a_timeout_without_a_decision_exits_two() {
    let stub = Stub::start(Behaviour::Pending).await;
    let output = run(&arguments("1"), Some(&stub.url), Some("token")).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "a hook nobody answers must deny: {}",
        said(&output)
    );
    assert!(
        said(&output).contains("expired"),
        "the reason is the missing decision: {}",
        said(&output)
    );
}

#[tokio::test]
async fn a_host_that_dies_while_the_hook_waits_exits_two() {
    let stub = Stub::start(Behaviour::DieAfterCreate).await;
    let output = run(&arguments("5"), Some(&stub.url), Some("token")).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "a host that disappears is a denial, never an error of the binary: {}",
        said(&output)
    );
}

#[tokio::test]
async fn a_malformed_answer_exits_two() {
    let stub = Stub::start(Behaviour::Malformed).await;
    let output = run(&arguments("5"), Some(&stub.url), Some("token")).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "an answer without an approval is a denial: {}",
        said(&output)
    );
}

#[tokio::test]
async fn an_unreachable_host_exits_two() {
    let output = run(&arguments("5"), Some("http://127.0.0.1:1"), Some("token")).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "a refused connection denies, it is not a usage error: {}",
        said(&output)
    );
}

#[tokio::test]
async fn a_missing_environment_exits_two() {
    let output = run(&arguments("1"), None, None).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unconfigured hook fails closed: {}",
        said(&output)
    );
}

#[tokio::test]
async fn an_invalid_argument_exits_two() {
    let args = vec!["--nope".to_string()];
    let output = run(&args, None, None).await;
    assert_eq!(
        output.status.code(),
        Some(2),
        "a bad invocation denies too, so no CLI runs the action: {}",
        said(&output)
    );
}
