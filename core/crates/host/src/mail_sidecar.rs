//! The client of the triage sidecar (F1, modo sombra): one `POST /v1/triage` over
//! loopback under one deadline, the same pattern as the scorer's `local_http`. The
//! sidecar answers with `Connection: close`, so the answer ends at the end of the stream.

use std::collections::BTreeMap;
use std::time::Duration;

use excubia_core_chat::local_endpoint;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const ROUTE: &str = "/v1/triage";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// Where the sidecar listens: `http://127.0.0.1:<port>` or `http://localhost:<port>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarEndpoint {
    host: String,
    port: u16,
}

impl SidecarEndpoint {
    /// The error names the expected form and never the URL it was given.
    pub fn parse(url: &str) -> Result<Self, String> {
        let (host, port) = local_endpoint(url).ok_or_else(|| {
            "--mail-sidecar needs http://127.0.0.1:<port> or http://localhost:<port>".to_string()
        })?;
        Ok(Self { host, port })
    }
}

/// Laya's answer: per question, the probability of each option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarAnswer {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    pub questions: String,
    pub answers: BTreeMap<String, BTreeMap<String, f64>>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SidecarError {
    #[error("the sidecar is not listening")]
    Unavailable,
    #[error("the sidecar did not answer within its deadline")]
    Timeout,
    #[error("the sidecar's answer is not a triage answer")]
    BadResponse,
}

pub struct Sidecar {
    endpoint: SidecarEndpoint,
    deadline: Duration,
}

impl Sidecar {
    pub fn new(endpoint: SidecarEndpoint, deadline: Duration) -> Self {
        Self { endpoint, deadline }
    }

    /// One ficha in, one answer out. Connect, write and read share one deadline.
    pub async fn triage<T: Serialize>(&self, ficha: &T) -> Result<SidecarAnswer, SidecarError> {
        match tokio::time::timeout(self.deadline, self.exchange(ficha)).await {
            Ok(answer) => answer,
            Err(_elapsed) => Err(SidecarError::Timeout),
        }
    }

    async fn exchange<T: Serialize>(&self, ficha: &T) -> Result<SidecarAnswer, SidecarError> {
        // Plain data: the encoding cannot fail, and a failure would be judged like a bad answer.
        let body =
            serde_json::to_vec(&Request { state: ficha }).map_err(|_| SidecarError::BadResponse)?;
        let head = format!(
            "POST {ROUTE} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.endpoint.host,
            self.endpoint.port,
            body.len()
        );
        let mut stream = TcpStream::connect((self.endpoint.host.as_str(), self.endpoint.port))
            .await
            .map_err(|_| SidecarError::Unavailable)?;
        stream
            .write_all(head.as_bytes())
            .await
            .map_err(|_| SidecarError::Unavailable)?;
        stream
            .write_all(&body)
            .await
            .map_err(|_| SidecarError::Unavailable)?;
        // One byte past the cap tells an oversized answer from one that fits.
        let mut raw = Vec::new();
        (&mut stream)
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut raw)
            .await
            .map_err(|_| SidecarError::Unavailable)?;
        if raw.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(SidecarError::BadResponse);
        }
        answer_of(&raw)
    }
}

#[derive(Serialize)]
struct Request<'a, T> {
    state: &'a T,
}

/// The JSON body of a 2xx answer. Anything else is not a triage answer.
fn answer_of(raw: &[u8]) -> Result<SidecarAnswer, SidecarError> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(SidecarError::BadResponse)?;
    let status_line = String::from_utf8_lossy(&raw[..split]);
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok());
    if !status.is_some_and(|code| (200..300).contains(&code)) {
        return Err(SidecarError::BadResponse);
    }
    serde_json::from_slice(&raw[split + 4..]).map_err(|_| SidecarError::BadResponse)
}
