//! Pairing: the one unauthenticated write, and the response it returns.

use std::time::Duration;

use serde_json::{json, Value};

use super::signed::{HttpError, CONNECT_TIMEOUT};

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
