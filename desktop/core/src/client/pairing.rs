use std::time::Duration;

use reqwest::{header, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use super::errors::{detail_fields, status_error};
use super::host::normalize_endpoint;
use crate::error::{CoreError, Result};
use crate::transport::Transport;

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

fn pairing_message(body: &[u8]) -> String {
    detail_fields(body)
        .message
        .filter(|message| message != "pairing_rejected")
        .unwrap_or_else(|| {
            "El PC rechazó el emparejamiento. Comprueba el código e inténtalo de nuevo.".to_string()
        })
}
