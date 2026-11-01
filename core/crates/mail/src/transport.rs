//! The HTTP leg, behind a trait so the mail logic never touches a socket and
//! every test runs against a scripted answer.
//!
//! `ReqwestTransport` follows no redirect (an `Authorization` header must not
//! travel to a new origin), bounds the whole exchange and refuses an oversized
//! answer. The upload-session calls (`put_range`, `get_upload_status`, and the
//! legacy `put_bytes`) carry no bearer token: the session URL is the credential.

use async_trait::async_trait;

use crate::error::MailError;
use crate::redact::Redacted;

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// One raw answer. Errors are not interpreted here: the auth and Graph layers
/// decide what a status means, and transport failures are already redacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json(&self) -> Result<serde_json::Value, MailError> {
        serde_json::from_str(&self.body).map_err(|_| {
            MailError::BadResponse(Redacted::new(format!(
                "the HTTP {} answer is not JSON",
                self.status
            )))
        })
    }
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpResponse, MailError>;

    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError>;

    async fn post_json(
        &self,
        url: &str,
        bearer: &str,
        body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError>;

    async fn patch_json(
        &self,
        url: &str,
        bearer: &str,
        body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError>;

    /// A POST with no body: Graph's action endpoints, such as `/send`.
    async fn post_action(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError>;

    async fn put_bytes(
        &self,
        url: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<HttpResponse, MailError>;

    /// One ranged chunk of an upload session. `content_range` is the full
    /// `bytes a-b/total` value Graph needs to place the chunk.
    async fn put_range(
        &self,
        url: &str,
        content_range: &str,
        bytes: Vec<u8>,
    ) -> Result<HttpResponse, MailError>;

    /// The session status, whose `nextExpectedRanges` says where to resume.
    async fn get_upload_status(&self, url: &str) -> Result<HttpResponse, MailError>;
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, MailError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .user_agent(concat!("herdr-notify-mail/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(transport_error)?;
        Ok(Self { client })
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<HttpResponse, MailError> {
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await.map_err(transport_error)?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(MailError::BadResponse(Redacted::new(
                "the HTTP answer is larger than 4 MiB",
            )));
        }
        let body = String::from_utf8(bytes.to_vec())
            .map_err(|_| MailError::BadResponse(Redacted::new("the HTTP answer is not UTF-8")))?;
        Ok(HttpResponse { status, body })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpResponse, MailError> {
        self.send(self.client.post(url).form(form)).await
    }

    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        self.send(self.client.get(url).bearer_auth(bearer)).await
    }

    async fn post_json(
        &self,
        url: &str,
        bearer: &str,
        body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError> {
        self.send(self.client.post(url).bearer_auth(bearer).json(body))
            .await
    }

    async fn patch_json(
        &self,
        url: &str,
        bearer: &str,
        body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError> {
        self.send(self.client.patch(url).bearer_auth(bearer).json(body))
            .await
    }

    async fn post_action(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        // Graph answers 411 to a bodiless POST that does not say `Content-Length: 0`.
        self.send(
            self.client
                .post(url)
                .bearer_auth(bearer)
                .header(reqwest::header::CONTENT_LENGTH, "0"),
        )
        .await
    }

    async fn put_bytes(
        &self,
        url: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<HttpResponse, MailError> {
        self.send(
            self.client
                .put(url)
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(bytes.to_vec()),
        )
        .await
    }

    async fn put_range(
        &self,
        url: &str,
        content_range: &str,
        bytes: Vec<u8>,
    ) -> Result<HttpResponse, MailError> {
        self.send(
            self.client
                .put(url)
                .header(reqwest::header::CONTENT_RANGE, content_range)
                .body(bytes),
        )
        .await
    }

    async fn get_upload_status(&self, url: &str) -> Result<HttpResponse, MailError> {
        self.send(self.client.get(url)).await
    }
}

/// The URL is dropped from the message: an upload session URL is a credential
/// and `reqwest` would otherwise print it in its `Display`.
fn transport_error(error: reqwest::Error) -> MailError {
    MailError::Transport(Redacted::new(error.without_url().to_string()))
}
