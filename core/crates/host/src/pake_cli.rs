//! `host pair-code --pake`: asks the running daemon to arm its one PAKE code and prints it.
//!
//! The code lives in the daemon's memory only (`PakeSlot`), so this process cannot mint it: it calls the
//! daemon's loopback admin route with the host hook token from the data dir. The code goes to stdout once and
//! is in no file; the token and the code are never part of an error message.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::paths;

const ADMIN_PATH: &str = "/v1/admin/pake-code";
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// An answer is a short JSON object; anything bigger is not the daemon.
const MAX_ANSWER_BYTES: usize = 16 * 1024;

/// What the daemon armed: the code and the link that holds it (secrets: print once, keep nowhere) and how long
/// it lives. `uri` is `None` for a daemon older than F3a.
pub struct ArmedCode {
    pub code: String,
    pub expires_in: u64,
    pub uri: Option<String>,
}

/// The host hook token of `data_dir`, read as the daemon wrote it. It is never created here: a data dir with
/// no token has no daemon to talk to.
pub fn read_hook_token(data_dir: &Path) -> Result<String> {
    let path = paths::hook_token_path(data_dir);
    let token = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "no hook token at {}: the daemon has not run with this --data-dir",
            path.display()
        )
    })?;
    let token = token.trim();
    if token.is_empty() {
        bail!("the hook token at {} is empty", path.display());
    }
    Ok(token.to_owned())
}

/// `POST /v1/admin/pake-code` on `addr` with `token`. A daemon that is not listening is an error that says so.
pub async fn arm_code(addr: SocketAddr, token: &str) -> Result<ArmedCode> {
    let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| anyhow::anyhow!("the daemon at {addr} did not answer in time"))?
        .with_context(|| {
            format!("the daemon is not running at {addr}: start it first, then ask for the code")
        })?;
    let request = format!(
        "POST {ADMIN_PATH} HTTP/1.1\r\nHost: {addr}\r\nX-HN-Hook-Token: {token}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .context("send the request to the daemon")?;
    let mut answer = Vec::new();
    tokio::time::timeout(
        IO_TIMEOUT,
        (&mut stream)
            .take(MAX_ANSWER_BYTES as u64)
            .read_to_end(&mut answer),
    )
    .await
    .map_err(|_| anyhow::anyhow!("the daemon at {addr} did not finish answering"))?
    .context("read the daemon's answer")?;
    parse_answer(&String::from_utf8_lossy(&answer))
}

fn parse_answer(response: &str) -> Result<ArmedCode> {
    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("the answer is not HTTP")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("the answer has no status")?;
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status != "200" {
        // The daemon's refusal is a stable word, never a secret.
        let detail = json["detail"].as_str().unwrap_or("no detail");
        bail!("the daemon refused the request ({status}: {detail})");
    }
    let code = json["code"]
        .as_str()
        .context("the answer has no code")?
        .to_owned();
    let expires_in = json["expires_in"]
        .as_u64()
        .context("the answer has no expires_in")?;
    let uri = json["uri"].as_str().map(str::to_owned);
    Ok(ArmedCode {
        code,
        expires_in,
        uri,
    })
}

/// The code as a person reads it: two groups of four. The device accepts it with or without the dash.
pub fn grouped(code: &str) -> String {
    match (code.get(..4), code.get(4..)) {
        (Some(first), Some(rest)) => format!("{first}-{rest}"),
        _ => code.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_answer_is_read() {
        let armed = parse_answer(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"code\":\"ABCDEFGH\",\"expires_in\":180}",
        )
        .expect("answer");
        assert_eq!((armed.code.as_str(), armed.expires_in), ("ABCDEFGH", 180));
        assert_eq!(armed.uri, None, "a daemon before F3a sends no uri");
        let armed = parse_answer(
            "HTTP/1.1 200 OK\r\n\r\n{\"code\":\"ABCDEFGH\",\"expires_in\":180,\"uri\":\"excubia://pair?v=2\"}",
        )
        .expect("answer");
        assert_eq!(armed.uri.as_deref(), Some("excubia://pair?v=2"));
    }

    #[test]
    fn a_refusal_names_the_status_and_no_secret() {
        let error = parse_answer("HTTP/1.1 401 Unauthorized\r\n\r\n{\"detail\":\"unauthorized\"}")
            .err()
            .expect("error")
            .to_string();
        assert!(
            error.contains("401") && error.contains("unauthorized"),
            "{error}"
        );
        assert!(parse_answer("garbage").is_err());
        assert!(parse_answer("HTTP/1.1 200 OK\r\n\r\n{}").is_err());
    }

    #[test]
    fn the_code_is_shown_in_two_groups() {
        assert_eq!(grouped("ABCDEFGH"), "ABCD-EFGH");
    }
}
