//! The signed HTTP client used by tests and the manual demo.
//!
//! Adapted from `desktop/core/src/client.rs` (MIT): same headers, same target
//! (`path?query` exactly as signed), same body hashing. The canonical string
//! is not copied â€” it is `hn_core_auth::canonical_string`, the same function
//! the server verifies with.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use hn_core_auth::canonical_string;
use rand_core::{OsRng, RngCore};
use serde_json::{json, Value};

use crate::identity::DemoIdentity;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
    http: reqwest::Client,
    base_url: String,
    identity: DemoIdentity,
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
        let body = body.unwrap_or_default();
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
            .header("X-HN-Device", self.identity.device_id())
            .header("X-HN-Timestamp", timestamp.to_string())
            .header("X-HN-Nonce", nonce)
            .header("X-HN-Signature", signature);
        if !body.is_empty() {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.text().await?;
        Ok((status, body))
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

    pub async fn state(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/state", None).await
    }

    pub async fn respond(&self, pane_id: &str, text: &str) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "text": text })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/respond"), Some(body))
            .await
    }

    pub async fn keys(
        &self,
        pane_id: &str,
        keys: &[&str],
        confirm: bool,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "keys": keys, "confirm": confirm })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/keys"), Some(body))
            .await
    }

    pub async fn read(&self, pane_id: &str, lines: u16) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "lines": lines })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/read"), Some(body))
            .await
    }

    pub async fn seen(&self, pane_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/seen"), None)
            .await
    }

    /// Start a conversation from the host's agent catalogue.
    pub async fn create_workspace(
        &self,
        agent: &str,
        cwd: &str,
        label: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "agent": agent,
            "cwd": cwd,
            "label": label,
        }))
        .expect("json");
        self.expect_ok("POST", "/v1/workspaces", Some(body)).await
    }

    /// The host's agent catalogue: names and availability only.
    pub async fn catalog(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/catalog", None).await
    }

    /// Stop a live agent; the pane leaves the state list.
    pub async fn stop(&self, pane_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/stop"), None)
            .await
    }

    pub async fn chat_bootstrap(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/bootstrap", None).await
    }

    pub async fn chat_conversations(&self, params: &[(&str, &str)]) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!("/v1/chat/conversations{}", signed_query(params)),
            None,
        )
        .await
    }

    pub async fn chat_create(
        &self,
        title: &str,
        project_id: Option<&str>,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "title": title,
            "project_id": project_id,
        }))
        .expect("json");
        self.expect_ok("POST", "/v1/chat/conversations", Some(body))
            .await
    }

    pub async fn chat_conversation(&self, conversation_id: &str) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!("/v1/chat/conversations/{conversation_id}"),
            None,
        )
        .await
    }

    pub async fn chat_update(
        &self,
        conversation_id: &str,
        body: Value,
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/update"),
            Some(serde_json::to_vec(&body).expect("json")),
        )
        .await
    }

    pub async fn chat_timeline(
        &self,
        conversation_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!(
                "/v1/chat/conversations/{conversation_id}/timeline{}",
                signed_query(params)
            ),
            None,
        )
        .await
    }

    pub async fn chat_messages(
        &self,
        conversation_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!(
                "/v1/chat/conversations/{conversation_id}/messages{}",
                signed_query(params)
            ),
            None,
        )
        .await
    }

    pub async fn chat_send(
        &self,
        conversation_id: &str,
        client_id: &str,
        text: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "client_id": client_id,
            "text": text,
        }))
        .expect("json");
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/messages"),
            Some(body),
        )
        .await
    }

    pub async fn chat_read(
        &self,
        conversation_id: &str,
        last_seq: i64,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({"last_seq": last_seq})).expect("json");
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/read"),
            Some(body),
        )
        .await
    }

    pub async fn chat_sections(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/sections", None).await
    }

    pub async fn chat_upsert_section(
        &self,
        id: Option<&str>,
        name: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({"id": id, "name": name})).expect("json");
        self.expect_ok("POST", "/v1/chat/sections", Some(body))
            .await
    }

    pub async fn devices(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/devices", None).await
    }

    pub async fn revoke(&self, device_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/devices/{device_id}/revoke"), None)
            .await
    }

    /// Open the SSE stream and collect raw text for `seconds`.
    pub async fn events(&self, seconds: u64) -> Result<String, HttpError> {
        self.events_with_last_id(seconds, None).await
    }

    /// Same stream, resuming the chat journal after `last_id` (the SSE `id` a
    /// previous chat frame carried).
    pub async fn events_with_last_id(
        &self,
        seconds: u64,
        last_id: Option<i64>,
    ) -> Result<String, HttpError> {
        let timestamp = unix_seconds();
        let nonce = random_nonce();
        let canonical =
            canonical_string("GET", "/v1/events", b"", timestamp as i64, &nonce).expect("valid");
        let signature = self.identity.sign(&canonical);
        let mut request = self
            .http
            .get(format!("{}/v1/events", self.base_url))
            .header("X-HN-Device", self.identity.device_id())
            .header("X-HN-Timestamp", timestamp.to_string())
            .header("X-HN-Nonce", nonce)
            .header("X-HN-Signature", signature)
            .header("Accept", "text/event-stream");
        if let Some(last_id) = last_id {
            request = request.header("Last-Event-ID", last_id.to_string());
        }
        let mut response = request.send().await.map_err(|error| HttpError {
            status: 0,
            body: error.to_string(),
        })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(HttpError { status, body });
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        let mut collected = String::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, response.chunk()).await {
                Ok(Ok(Some(chunk))) => collected.push_str(&String::from_utf8_lossy(&chunk)),
                Ok(Ok(None)) | Ok(Err(_)) | Err(_) => break,
            }
        }
        Ok(collected)
    }
}

/// `POST /v1/pair`: the only unauthenticated write. Code, recovery code or
/// tailnet ownership is the authorization; this function just carries it.
pub async fn pair_device(
    base_url: &str,
    curve: &str,
    public_key: &str,
    label: &str,
    code: &str,
) -> Result<PairResponse, HttpError> {
    let http = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("http client");
    let mut body = json!({ "curve": curve, "public_key": public_key });
    if !label.trim().is_empty() {
        body["label"] = Value::String(label.trim().to_string());
    }
    if !code.trim().is_empty() {
        body["code"] = Value::String(code.trim().to_string());
    }
    let response = http
        .post(format!("{}/v1/pair", base_url.trim_end_matches('/')))
        .timeout(Duration::from_secs(20))
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(&body).expect("json"))
        .send()
        .await
        .map_err(|error| HttpError {
            status: 0,
            body: error.to_string(),
        })?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(HttpError { status, body });
    }
    serde_json::from_str(&body).map_err(|error| HttpError {
        status,
        body: format!("{error}: {body}"),
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PairResponse {
    pub device_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub recovery_code: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
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

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same golden vectors the gateway, the Kotlin app and the desktop
    /// client pin: all sides must produce identical bytes.
    #[test]
    fn canonicalisation_matches_the_gateway_vectors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../gateway/tests/vectors/signing_vectors.json");
        let raw = std::fs::read_to_string(&path).expect("gateway vectors");
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
        assert!(checked >= 6, "all gateway vectors must be checked");
    }
}
