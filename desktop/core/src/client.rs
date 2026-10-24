use std::time::Duration;

use reqwest::{header, Client, Method, RequestBuilder, Response, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{CoreError, Result};
use crate::identity::DeviceIdentity;
use crate::signing;
use crate::state::StateSnapshot;
use crate::transport::{plain_host_is_safe, Transport};

const READ_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Deserialize)]
pub struct PairResponse {
    pub device_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub recovery_code: Option<String>,
}

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
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    agents: Vec<CatalogAgent>,
}

#[derive(Debug, Default)]
struct DetailFields {
    code: Option<String>,
    field: Option<String>,
    message: Option<String>,
}

/// A signed client bound to one device identity.
#[derive(Clone)]
pub struct GatewayClient {
    http: Client,
    base_url: String,
    identity: DeviceIdentity,
}

impl GatewayClient {
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
                "el gateway no confirmó la acción {name}"
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
                "el gateway respondió por otro panel: {}",
                read.pane_id
            )))
        }
    }

    fn url(&self, path: &str) -> Result<Url> {
        Url::parse(&format!("{}{path}", self.base_url))
            .map_err(|error| CoreError::InvalidInput(format!("URL del gateway: {error}")))
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
        let mut url = self.url(path)?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (name, value) in query {
                pairs.append_pair(name, value);
            }
        }
        let mut request = self
            .sign(method, &url, body.unwrap_or_default())?
            .timeout(READ_TIMEOUT);
        if let Some(body) = body {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body.to_vec());
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

/// The only unauthenticated write. Code, recovery code or tailnet ownership is
/// the authorization; this function just carries the request.
pub async fn pair_device(
    base_url: &str,
    transport: &Transport,
    curve: &str,
    public_key: &str,
    label: &str,
    code: &str,
) -> Result<PairResponse> {
    let base_url = normalize_endpoint(base_url, transport)?;
    let http = transport.client()?;

    let mut body = json!({
        "curve": curve,
        "public_key": public_key,
    });
    if !label.trim().is_empty() {
        body["label"] = Value::String(label.trim().to_string());
    }
    if !code.trim().is_empty() {
        body["code"] = Value::String(code.trim().to_string());
    }
    let body = serde_json::to_vec(&body)?;

    let response = http
        .post(format!("{base_url}/v1/pair"))
        .timeout(Duration::from_secs(20))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await?;
    let status = response.status();
    let bytes = response.bytes().await?;

    if status == StatusCode::FORBIDDEN {
        return Err(CoreError::PairingRejected(pairing_message(&bytes)));
    }
    if !status.is_success() {
        return Err(status_error(status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(CoreError::from)
}

pub fn normalize_base_url(raw: &str) -> Result<String> {
    normalize_endpoint(raw, &Transport::Plain)
}

/// The base URL for a transport. A URL with no scheme is clear HTTP, so it
/// cannot name a pinned host; clear HTTP is refused outside loopback and the
/// tailnet.
pub fn normalize_endpoint(raw: &str, transport: &Transport) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput(
            "escribe la URL del gateway".to_string(),
        ));
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let mut url = Url::parse(&with_scheme)
        .map_err(|error| CoreError::InvalidInput(format!("URL del gateway: {error}")))?;
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

pub fn validate_device_id(value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(CoreError::InvalidInput(
            "el identificador debe tener entre 1 y 64 caracteres".to_string(),
        ));
    }
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(CoreError::InvalidInput(
            "el identificador solo admite letras, números, '-' y '_'".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_pane_id(pane_id: &str) -> Result<()> {
    let Some((workspace, pane)) = pane_id.split_once(":p") else {
        return Err(CoreError::InvalidInput("panel inválido".to_string()));
    };
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    };
    if !valid(workspace) || !valid(pane) {
        return Err(CoreError::InvalidInput("panel inválido".to_string()));
    }
    Ok(())
}

async fn expect_json<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(CoreError::Unauthorized);
    }
    if status == StatusCode::FORBIDDEN {
        return Err(CoreError::Forbidden);
    }
    if !status.is_success() {
        return Err(status_error(status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(CoreError::from)
}

pub(crate) fn status_error(status: StatusCode, body: &[u8]) -> CoreError {
    if status == StatusCode::UNAUTHORIZED {
        return CoreError::Unauthorized;
    }
    if status == StatusCode::FORBIDDEN {
        return CoreError::Forbidden;
    }
    let detail = detail_fields(body);
    CoreError::Status {
        status: status.as_u16(),
        message: detail
            .message
            .or_else(|| detail.code.clone())
            .or_else(|| status.canonical_reason().map(str::to_string))
            .unwrap_or_else(|| "sin detalle".to_string()),
        code: detail.code,
        field: detail.field,
    }
}

/// `detail` is a string, an action `{code, message}` object, or a `422`
/// `{code, errors: [{loc, msg}]}` body.
fn detail_fields(body: &[u8]) -> DetailFields {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return DetailFields::default();
    };
    match value.get("detail") {
        Some(Value::String(text)) => DetailFields {
            message: Some(text.clone()),
            ..DetailFields::default()
        },
        Some(Value::Object(fields)) => {
            let code = fields
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_string);
            let message = fields
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string);
            let first = fields
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first());
            // A dedicated rejection carries `field` next to `code`; a body
            // validation error carries `errors[0].loc[0]`.
            let field = fields
                .get("field")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    first
                        .and_then(|error| error.get("loc"))
                        .and_then(Value::as_array)
                        .and_then(|loc| loc.first())
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            let message = message.or_else(|| {
                first
                    .and_then(|error| error.get("msg"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            DetailFields {
                code,
                field,
                message,
            }
        }
        _ => DetailFields::default(),
    }
}

fn pairing_message(body: &[u8]) -> String {
    detail_fields(body)
        .message
        .filter(|message| message != "pairing_rejected")
        .unwrap_or_else(|| {
            "El gateway rechazó el emparejamiento. Comprueba el código e inténtalo de nuevo."
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use hn_core_pin::PinSet;

    use super::*;

    #[test]
    fn base_url_gets_a_scheme_and_loses_trailing_slashes() {
        assert_eq!(
            normalize_base_url("127.0.0.1:8787/").unwrap(),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            normalize_base_url("http://mi-pc.tail1234.ts.net:8787/base/").unwrap(),
            "http://mi-pc.tail1234.ts.net:8787/base"
        );
        assert!(normalize_base_url("  ").is_err());
        assert!(normalize_base_url("ftp://host").is_err());
    }

    #[test]
    fn clear_http_is_refused_on_a_lan_address() {
        assert!(normalize_base_url("http://192.168.1.20:8787").is_err());
        assert!(normalize_base_url("192.168.1.20:8787").is_err());
        assert!(normalize_base_url("https://192.168.1.20:8443").is_ok());
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
    fn pane_ids_follow_the_gateway_regex() {
        assert!(validate_pane_id("w1:p3").is_ok());
        assert!(validate_pane_id("wQ:p3N").is_ok());
        assert!(validate_pane_id("w1").is_err());
        assert!(validate_pane_id("w-1:p3").is_err());
        assert!(validate_pane_id("herdr agent").is_err());
    }

    #[test]
    fn a_dedicated_422_field_is_read() {
        let fields =
            detail_fields(br#"{"detail":{"code":"unknown_agent","field":"agent","message":"no"}}"#);
        assert_eq!(fields.field.as_deref(), Some("agent"));
        assert_eq!(fields.message.as_deref(), Some("no"));

        let validation = detail_fields(
            br#"{"detail":{"code":"invalid_body","errors":[{"loc":["cwd"],"msg":"bad"}]}}"#,
        );
        assert_eq!(validation.field.as_deref(), Some("cwd"));
        assert_eq!(validation.message.as_deref(), Some("bad"));
    }

    #[test]
    fn device_ids_are_safe_for_a_header() {
        assert!(validate_device_id("dev-0123456789abcdef").is_ok());
        assert!(validate_device_id("hn-test_1").is_ok());
        assert!(validate_device_id("con espacio").is_err());
        assert!(validate_device_id("").is_err());
    }
}
