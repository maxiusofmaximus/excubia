//! The loopback HTTP client the seat's own processes use.
//!
//! Two of them talk to the host: the MCP shim (with the per-run tool token)
//! and the UAC hook (with the host token). Both speak plain HTTP over
//! loopback, so one small client serves both and the loopback rule is
//! enforced in a single place.

use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How long the connection to the host may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The largest answer the host may send.
pub const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// A plain-HTTP loopback URL: `http://host:port/path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopbackUrl {
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl LoopbackUrl {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let rest = raw
            .strip_prefix("http://")
            .ok_or_else(|| "the URL must use plain http on loopback".to_string())?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host.to_string(),
                port.parse::<u16>()
                    .map_err(|_| "the URL has an invalid port".to_string())?,
            ),
            None => (authority.to_string(), 80),
        };
        if host.is_empty() {
            return Err("the URL has no host".to_string());
        }
        Ok(Self {
            host,
            port,
            path: path.to_string(),
        })
    }

    /// The same host and port with another path.
    pub fn at(&self, path: &str) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            path: path.to_string(),
        }
    }
}

/// One answer of the host.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The body as JSON. An empty body is an empty object.
    pub fn json(&self) -> Result<Value, String> {
        if self.body.is_empty() {
            return Ok(Value::Object(Default::default()));
        }
        serde_json::from_slice(&self.body).map_err(|error| format!("invalid JSON: {error}"))
    }
}

/// Send one request and read the whole answer.
pub async fn request(
    url: &LoopbackUrl,
    method: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    timeout: Duration,
) -> Result<HttpResponse, String> {
    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {}\r\n",
        url.path,
        url.host,
        url.port,
        body.len()
    );
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");

    let exchange = async {
        let stream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect((url.host.as_str(), url.port)),
        )
        .await
        .map_err(|_| "the host connection timed out".to_string())?
        .map_err(|error| format!("cannot reach the host: {error}"))?;
        let peer = stream
            .peer_addr()
            .map_err(|error| format!("cannot inspect peer: {error}"))?;
        if !peer.ip().is_loopback() {
            return Err("the host endpoint is not loopback".to_string());
        }
        let mut stream = stream;
        stream
            .write_all(head.as_bytes())
            .await
            .map_err(|error| format!("cannot write the request: {error}"))?;
        stream
            .write_all(body)
            .await
            .map_err(|error| format!("cannot write the body: {error}"))?;
        let mut response = Vec::new();
        stream
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut response)
            .await
            .map_err(|error| format!("cannot read the answer: {error}"))?;
        Ok(response)
    };

    let raw = tokio::time::timeout(timeout, exchange)
        .await
        .map_err(|_| "the request timed out".to_string())??;
    parse_response(&raw)
}

/// Split a raw HTTP answer into its status and its body.
pub fn parse_response(raw: &[u8]) -> Result<HttpResponse, String> {
    let separator = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "the host answered without HTTP headers".to_string())?;
    let head = String::from_utf8_lossy(&raw[..separator]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| "the host answered with an invalid status".to_string())?;
    Ok(HttpResponse {
        status,
        body: raw[separator + 4..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_refuses_anything_but_plain_http() {
        assert!(LoopbackUrl::parse("https://127.0.0.1:1/x").is_err());
        assert!(LoopbackUrl::parse("http://:1/x").is_err());
        assert!(LoopbackUrl::parse("http://127.0.0.1:notaport/x").is_err());
    }

    #[test]
    fn the_parser_keeps_the_host_the_port_and_the_path() {
        let url = LoopbackUrl::parse("http://127.0.0.1:4242/internal/chat/tools").expect("url");
        assert_eq!(url.host, "127.0.0.1");
        assert_eq!(url.port, 4242);
        assert_eq!(url.path, "/internal/chat/tools");
        assert_eq!(url.at("/v1/chat/approvals").path, "/v1/chat/approvals");
        let bare = LoopbackUrl::parse("http://127.0.0.1").expect("url");
        assert_eq!(bare.path, "/");
        assert_eq!(bare.port, 80);
    }

    #[test]
    fn the_status_and_the_body_are_split() {
        let body = br#"{"ok":true}"#;
        let raw = [
            b"HTTP/1.1 201 Created\r\nContent-Length: ".as_slice(),
            body.len().to_string().as_bytes(),
            b"\r\n\r\n",
            body,
        ]
        .concat();
        let response = parse_response(&raw).expect("response");
        assert_eq!(response.status, 201);
        assert_eq!(response.json().expect("json")["ok"], true);
        assert!(parse_response(b"not http").is_err());
    }
}
