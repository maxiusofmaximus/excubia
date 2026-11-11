use std::time::Duration;

use reqwest::{header, Client, Method, RequestBuilder, Response, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::errors::{expect_json, status_error};
use super::validate::validate_pane_id;
use crate::error::{CoreError, Result};
use crate::identity::DeviceIdentity;
use crate::signing;
use crate::state::StateSnapshot;
use crate::transport::{plain_host_is_safe, Transport};

const READ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct ReadResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    pane_id: String,
}

#[derive(Debug, Deserialize)]
struct ActionResponse {
    #[serde(default)]
    ok: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceCreated {
    pub workspace_id: String,
    pub pane_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogAgent {
    pub name: String,
    #[serde(default)]
    pub available: bool,
    /// An older host does not send this; it was always startable then.
    #[serde(default = "startable_by_default")]
    pub runnable: bool,
    /// What the host says is wrong, in Spanish. Never a path.
    #[serde(default)]
    pub problem: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// The PC's default agent for new conversations; an older host never sends it.
    #[serde(default)]
    pub is_default: bool,
}

fn startable_by_default() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    agents: Vec<CatalogAgent>,
}

/// A signed client bound to one device identity.
#[derive(Clone)]
pub struct HostClient {
    http: Client,
    base_url: String,
    identity: DeviceIdentity,
}

impl HostClient {
    /// The device this client signs as: the operator grants it `APPROVE`.
    pub fn device_id(&self) -> &str {
        self.identity.device_id()
    }

    pub fn new(base_url: &str, identity: DeviceIdentity) -> Result<Self> {
        Self::with_transport(base_url, identity, &Transport::Plain)
    }

    pub fn with_transport(
        base_url: &str,
        identity: DeviceIdentity,
        transport: &Transport,
    ) -> Result<Self> {
        Ok(Self {
            http: transport.client()?,
            base_url: normalize_endpoint(base_url, transport)?,
            identity,
        })
    }

    pub async fn state(&self) -> Result<StateSnapshot> {
        let url = self.url("/v1/state")?;
        let request = self.sign(Method::GET, &url, b"")?.timeout(READ_TIMEOUT);
        let mut state: StateSnapshot = expect_json(request.send().await?).await?;
        state.sanitize();
        Ok(state)
    }

    pub async fn events(&self) -> Result<Response> {
        let url = self.url("/v1/events")?;
        let response = self
            .sign(Method::GET, &url, b"")?
            .header(header::ACCEPT, "text/event-stream")
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.bytes().await?;
        Err(status_error(status, &body))
    }

    /// The host's closed catalogue. Only names and availability: never a
    /// command, its arguments or a path.
    pub async fn catalog(&self) -> Result<Vec<CatalogAgent>> {
        let url = self.url("/v1/catalog")?;
        let request = self.sign(Method::GET, &url, b"")?.timeout(READ_TIMEOUT);
        let response: CatalogResponse = expect_json(request.send().await?).await?;
        Ok(response.agents)
    }

    /// Stops a live agent. A pane that is already gone answers `pane_gone`.
    pub async fn stop_agent(&self, pane_id: &str) -> Result<()> {
        self.action("stop", pane_id, Vec::new()).await
    }

    /// Makes `name` the PC's default agent for new conversations (needs APPROVE).
    pub async fn set_default(&self, name: &str) -> Result<()> {
        let body = serde_json::to_vec(&json!({ "name": name }))?;
        let url = self.url("/v1/catalog/default")?;
        let request = self
            .sign(Method::POST, &url, &body)?
            .timeout(READ_TIMEOUT)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body);
        let _: Value = expect_json(request.send().await?).await?;
        Ok(())
    }

    /// Starts one agent from the host's closed catalogue in `cwd`.
    pub async fn create_workspace(
        &self,
        agent: &str,
        cwd: &str,
        label: Option<&str>,
    ) -> Result<WorkspaceCreated> {
        let mut body = json!({ "agent": agent, "cwd": cwd });
        if let Some(label) = label.map(str::trim).filter(|label| !label.is_empty()) {
            body["label"] = Value::String(label.to_string());
        }
        let body = serde_json::to_vec(&body)?;
        let url = self.url("/v1/workspaces")?;
        let request = self
            .sign(Method::POST, &url, &body)?
            .timeout(READ_TIMEOUT)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body);
        expect_json(request.send().await?).await
    }

    pub async fn respond(&self, pane_id: &str, text: &str) -> Result<()> {
        let body = serde_json::to_vec(&json!({ "text": text }))?;
        self.action("respond", pane_id, body).await
    }

    pub async fn send_keys(&self, pane_id: &str, keys: &[String], confirm: bool) -> Result<()> {
        let body = serde_json::to_vec(&json!({ "keys": keys, "confirm": confirm }))?;
        self.action("keys", pane_id, body).await
    }

    /// `seen` is an acknowledged no-op: the phone marks locally, herdr is not
    /// touched. The call still authenticates and validates the pane.
    pub async fn mark_seen(&self, pane_id: &str) -> Result<()> {
        self.action("seen", pane_id, Vec::new()).await
    }

    async fn action(&self, name: &str, pane_id: &str, body: Vec<u8>) -> Result<()> {
        validate_pane_id(pane_id)?;
        let url = self.url(&format!("/v1/agents/{pane_id}/{name}"))?;
        let mut request = self.sign(Method::POST, &url, &body)?.timeout(READ_TIMEOUT);
        if !body.is_empty() {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let response: ActionResponse = expect_json(request.send().await?).await?;
        if response.ok {
            Ok(())
        } else {
            Err(CoreError::Protocol(format!(
                "el PC no confirmó la acción {name}"
            )))
        }
    }

    pub async fn read_output(&self, pane_id: &str, lines: u16) -> Result<String> {
        validate_pane_id(pane_id)?;
        let body = serde_json::to_vec(&json!({ "lines": lines.clamp(1, 200) }))?;
        let url = self.url(&format!("/v1/agents/{pane_id}/read"))?;
        let request = self
            .sign(Method::POST, &url, &body)?
            .timeout(READ_TIMEOUT)
            .body(body);
        let read: ReadResponse = expect_json(request.send().await?).await?;
        if read.pane_id.is_empty() || read.pane_id == pane_id {
            Ok(read.text)
        } else {
            Err(CoreError::Protocol(format!(
                "el PC respondió por otro panel: {}",
                read.pane_id
            )))
        }
    }

    fn url(&self, path: &str) -> Result<Url> {
        Url::parse(&format!("{}{path}", self.base_url))
            .map_err(|error| CoreError::InvalidInput(format!("Dirección del PC: {error}")))
    }

    /// One signed request for the modules that own their own wire shapes
    /// (chat). The signature covers the final path and query.
    pub(crate) fn signed_call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&[u8]>,
    ) -> Result<RequestBuilder> {
        self.signed_typed(
            method,
            path,
            query,
            body.map(|bytes| (bytes, "application/json")),
        )
    }

    /// `signed_call` for a body of another media type (a voice note is `audio/wav`).
    pub(crate) fn signed_typed(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<(&[u8], &str)>,
    ) -> Result<RequestBuilder> {
        let mut url = self.url(path)?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (name, value) in query {
                pairs.append_pair(name, value);
            }
        }
        let bytes = body.map(|(bytes, _)| bytes).unwrap_or_default();
        let mut request = self.sign(method, &url, bytes)?.timeout(READ_TIMEOUT);
        if let Some((bytes, content_type)) = body {
            request = request
                .header(header::CONTENT_TYPE, content_type)
                .body(bytes.to_vec());
        }
        Ok(request)
    }

    fn sign(&self, method: Method, url: &Url, body: &[u8]) -> Result<RequestBuilder> {
        let target = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_string(),
        };
        let timestamp = signing::unix_seconds()?;
        let nonce = signing::random_nonce();
        let canonical =
            signing::canonical_string(method.as_str(), &target, body, timestamp, &nonce);
        let signature = self.identity.sign_canonical(canonical.as_bytes());

        Ok(self
            .http
            .request(method, url.clone())
            .header("X-HN-Device", self.identity.device_id())
            .header("X-HN-Timestamp", timestamp.to_string())
            .header("X-HN-Nonce", nonce)
            .header("X-HN-Signature", signature))
    }
}

/// The base URL for a transport. A URL with no scheme is clear HTTP, so it
/// cannot name a pinned host; clear HTTP is refused outside loopback and the
/// tailnet.
pub fn normalize_endpoint(raw: &str, transport: &Transport) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput(
            "escribe la dirección del PC".to_string(),
        ));
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let mut url = Url::parse(&with_scheme)
        .map_err(|error| CoreError::InvalidInput(format!("Dirección del PC: {error}")))?;
    let host = url
        .host_str()
        .filter(|_| matches!(url.scheme(), "http" | "https"))
        .ok_or_else(|| {
            CoreError::InvalidInput("la URL debe ser http:// o https:// con un host".to_string())
        })?;
    if !transport.accepts_scheme(url.scheme()) {
        return Err(CoreError::InvalidInput(
            "un enlace de emparejamiento solo usa https://".to_string(),
        ));
    }
    if url.scheme() == "http" && !plain_host_is_safe(host) {
        return Err(CoreError::InvalidInput(
            "sin cifrar solo llega a loopback o a la tailnet; para la red local usa el enlace de emparejamiento del host".to_string(),
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use excubia_core_pin::PinSet;

    use super::*;

    #[test]
    fn base_url_gets_a_scheme_and_loses_trailing_slashes() {
        assert_eq!(
            normalize_endpoint("127.0.0.1:8787/", &Transport::Plain).unwrap(),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            normalize_endpoint("http://mi-pc.tail1234.ts.net:8787/base/", &Transport::Plain)
                .unwrap(),
            "http://mi-pc.tail1234.ts.net:8787/base"
        );
        assert!(normalize_endpoint("  ", &Transport::Plain).is_err());
        assert!(normalize_endpoint("ftp://host", &Transport::Plain).is_err());
    }

    #[test]
    fn clear_http_is_refused_on_a_lan_address() {
        assert!(normalize_endpoint("http://192.168.1.20:8787", &Transport::Plain).is_err());
        assert!(normalize_endpoint("192.168.1.20:8787", &Transport::Plain).is_err());
        assert!(normalize_endpoint("https://192.168.1.20:8443", &Transport::Plain).is_ok());
    }

    #[test]
    fn a_pinned_endpoint_is_https_only() {
        let pins = PinSet::new(
            "-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI"
                .parse()
                .unwrap(),
            None,
        );
        let pinned = Transport::Pinned(pins);
        assert_eq!(
            normalize_endpoint("https://192.168.1.20:8443/", &pinned).unwrap(),
            "https://192.168.1.20:8443"
        );
        assert!(normalize_endpoint("http://192.168.1.20:8443", &pinned).is_err());
    }

    #[test]
    fn the_catalogue_marks_the_default_and_an_older_host_has_none() {
        let body = r#"{"agents":[
            {"name":"claude","available":true,"runnable":true,"is_default":true},
            {"name":"codex","available":true,"runnable":true}
        ]}"#;
        let parsed: CatalogResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.agents[0].is_default);
        assert!(!parsed.agents[1].is_default);
    }
}
