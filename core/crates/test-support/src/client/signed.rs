//! Signed requests: the transport, the signing and the shared error type.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use excubia_auth::canonical_string;
use rand_core::{OsRng, RngCore};
use serde_json::Value;

use crate::identity::DemoIdentity;

pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub body: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "HTTP {}: {}", self.status, self.body)
    }
}

impl std::error::Error for HttpError {}

#[derive(Clone)]
pub struct SignedClient {
    pub(super) http: reqwest::Client,
    pub(super) base_url: String,
    pub(super) identity: DemoIdentity,
}

impl SignedClient {
    pub fn new(base_url: &str, identity: DemoIdentity) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("http client");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            identity,
        }
    }

    pub fn identity(&self) -> &DemoIdentity {
        &self.identity
    }

    /// Send one signed request and return (status, body) whatever the status.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, String), reqwest::Error> {
        let (status, _, body) = self.request_with_retry_after(method, path, body).await?;
        Ok((status, body))
    }

    /// As [`Self::request`], and the `Retry-After` header of the answer, if it has one.
    pub async fn request_with_retry_after(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Option<String>, String), reqwest::Error> {
        let (status, headers, body) = self.request_raw(method, path, body).await?;
        let retry_after = headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok((
            status,
            retry_after,
            String::from_utf8_lossy(&body).into_owned(),
        ))
    }

    /// Send one signed request and return the status, the headers and the raw body.
    pub async fn request_raw(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(u16, reqwest::header::HeaderMap, Vec<u8>), reqwest::Error> {
        self.send(method, path, "application/json", body.unwrap_or_default())
            .await
    }

    /// As [`Self::request_raw`], with the body sent as `text/calendar`.
    pub async fn request_calendar(
        &self,
        method: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Result<(u16, reqwest::header::HeaderMap, Vec<u8>), reqwest::Error> {
        self.send(method, path, "text/calendar; charset=utf-8", body)
            .await
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<(u16, reqwest::header::HeaderMap, Vec<u8>), reqwest::Error> {
        let timestamp = unix_seconds();
        let nonce = random_nonce();
        let canonical = canonical_string(method, path, &body, timestamp as i64, &nonce)
            .expect("valid request parts");
        let signature = self.identity.sign(&canonical);

        let mut request = self
            .http
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).expect("method"),
                format!("{}{path}", self.base_url),
            )
            .header("X-Excubia-Device", self.identity.device_id())
            .header("X-Excubia-Timestamp", timestamp.to_string())
            .header("X-Excubia-Nonce", nonce)
            .header("X-Excubia-Signature", signature);
        if !body.is_empty() {
            request = request.header("Content-Type", content_type).body(body);
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        Ok((status, headers, body.to_vec()))
    }

    pub(crate) async fn expect_ok(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Value, HttpError> {
        let (status, body) = self
            .request(method, path, body)
            .await
            .map_err(|error| HttpError {
                status: 0,
                body: error.to_string(),
            })?;
        if !(200..300).contains(&status) {
            return Err(HttpError { status, body });
        }
        Ok(serde_json::from_str(&body).unwrap_or(Value::Null))
    }
}

/// `path?k=v&â€¦` with RFC 3986 unreserved characters only: the exact target is
/// what the signature covers, so the client must build it deterministically.
pub(crate) fn signed_query(params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = params
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect();
    format!("?{}", pairs.join("&"))
}

fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(super) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(super) fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared golden vectors every client pins: all sides must produce
    /// identical bytes.
    #[test]
    fn canonicalisation_matches_the_shared_vectors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/contracts/vectors/signing_vectors.json");
        let raw = std::fs::read_to_string(&path).expect("shared vectors");
        let vectors: Value = serde_json::from_str(&raw).expect("json");
        let mut checked = 0;
        for vector in vectors["vectors"].as_array().expect("vectors") {
            let method = vector["method"].as_str().expect("method");
            let target = vector["target"].as_str().expect("target");
            let body = vector["body"].as_str().expect("body").as_bytes();
            let timestamp = vector["timestamp"].as_i64().expect("timestamp");
            let nonce = vector["nonce"].as_str().expect("nonce");
            let expected = vector["expected"].as_str().expect("expected");
            let canonical =
                canonical_string(method, target, body, timestamp, nonce).expect("canonical string");
            assert_eq!(canonical, expected, "vector {}", vector["name"]);
            checked += 1;
        }
        assert!(checked >= 6, "all shared vectors must be checked");
    }
}
