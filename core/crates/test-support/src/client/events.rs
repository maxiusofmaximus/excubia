//! The SSE event stream.

use std::time::Duration;

use excubia_core_auth::canonical_string;

use super::signed::{random_nonce, unix_seconds, HttpError, SignedClient};

impl SignedClient {
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
            .header("X-Excubia-Device", self.identity.device_id())
            .header("X-Excubia-Timestamp", timestamp.to_string())
            .header("X-Excubia-Nonce", nonce)
            .header("X-Excubia-Signature", signature)
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
