//! OAuth 2.0 device-code login against Microsoft Entra ID.
//!
//! The permission list is closed in [`SCOPES`] and checked before every login
//! and refresh: the product never asks for `Mail.Send`. Device codes, access
//! tokens and refresh tokens live in [`Secret`], so a `Debug` of any value of
//! this module is safe to log.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, Instant};

use crate::error::MailError;
use crate::redact::{Redacted, Secret};
use crate::transport::HttpTransport;

/// The one authority every request of this crate uses.
pub const AUTHORITY: &str = "https://login.microsoftonline.com";

/// The closed permission list (spec rule 1). Never add `Mail.Send`: there is no
/// send path, and a test fails if the scope ever appears in a login request.
pub const SCOPES: [&str; 3] = ["offline_access", "User.Read", "Mail.ReadWrite"];

/// The permission list of the send consent (MAIL2A). It is requested only by
/// [`start_send_consent`], on its own device login, and never joins [`SCOPES`].
pub const SEND_SCOPES: [&str; 2] = ["offline_access", "Mail.Send"];

const FORBIDDEN_SCOPE: &str = "mail.send";
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const DEFAULT_INTERVAL_SECS: u64 = 5;
const SLOW_DOWN_STEP_SECS: u64 = 5;
const MAX_LOGIN_SECS: u64 = 3600;

/// The three authorities the spec allows. Personal accounts must use
/// `consumers`: `common` is known to fail for them (verified 2026-10-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tenant {
    Consumers,
    Organizations,
    Common,
}

impl Tenant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Consumers => "consumers",
            Self::Organizations => "organizations",
            Self::Common => "common",
        }
    }

    pub fn parse(value: &str) -> Result<Self, MailError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "consumers" => Ok(Self::Consumers),
            "organizations" => Ok(Self::Organizations),
            "common" => Ok(Self::Common),
            other => Err(MailError::InvalidConfig(Redacted::new(format!(
                "unknown tenant \"{other}\"; use consumers, organizations or common"
            )))),
        }
    }
}

impl fmt::Display for Tenant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Refuse to build any request if the permission list ever drifts. Called by
/// configuration loading, device login and refresh; the tests call it too.
pub fn assert_scopes_safe() -> Result<(), MailError> {
    if let Some(scope) = SCOPES
        .iter()
        .find(|scope| scope.to_ascii_lowercase().starts_with(FORBIDDEN_SCOPE))
    {
        return Err(MailError::InvalidConfig(Redacted::new(format!(
            "the scope {scope} is forbidden: this product never sends mail"
        ))));
    }
    let closed = SCOPES.len() == 3
        && ["offline_access", "User.Read", "Mail.ReadWrite"]
            .iter()
            .all(|required| SCOPES.contains(required));
    if !closed {
        return Err(MailError::InvalidConfig(Redacted::new(
            "the permission list must be exactly offline_access, User.Read and Mail.ReadWrite",
        )));
    }
    Ok(())
}

/// The send list must stay exactly the two scopes: nothing that reads mail.
pub fn assert_send_scopes_safe() -> Result<(), MailError> {
    let closed = SEND_SCOPES.len() == 2
        && SEND_SCOPES.contains(&"offline_access")
        && SEND_SCOPES.contains(&"Mail.Send");
    if !closed {
        return Err(MailError::InvalidConfig(Redacted::new(
            "the send permission list must be exactly offline_access and Mail.Send",
        )));
    }
    Ok(())
}

fn endpoint(tenant: Tenant, tail: &str) -> String {
    format!("{AUTHORITY}/{}/{tail}", tenant.as_str())
}

/// What the user has to type on the Microsoft page. The device code is kept
/// private: only [`poll_device_login`] reads it.
#[derive(Clone)]
pub struct DeviceLogin {
    pub label: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    device_code: Secret,
}

impl fmt::Debug for DeviceLogin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceLogin")
            .field("label", &self.label)
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .field("device_code", &self.device_code)
            .finish()
    }
}

impl DeviceLogin {
    pub(crate) fn new(
        label: String,
        user_code: String,
        verification_uri: String,
        expires_in: u64,
        interval: u64,
        device_code: Secret,
    ) -> Self {
        Self {
            label,
            user_code,
            verification_uri,
            expires_in,
            interval,
            device_code,
        }
    }
}

/// Tokens as the store keeps them. `Debug` shows only markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    access_token: Secret,
    refresh_token: Option<Secret>,
    expires_in: u64,
}

impl Tokens {
    pub fn new(
        access_token: impl Into<String>,
        refresh_token: Option<Secret>,
        expires_in: u64,
    ) -> Self {
        Self {
            access_token: Secret::new(access_token),
            refresh_token,
            expires_in,
        }
    }

    pub fn access_token(&self) -> &str {
        self.access_token.expose()
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_ref().map(Secret::expose)
    }

    pub fn expires_in(&self) -> u64 {
        self.expires_in
    }

    fn from_body(body: &Value, fallback_refresh: Option<&str>) -> Result<Self, MailError> {
        let raw: RawTokenResponse = serde_json::from_value(body.clone())
            .map_err(|_| MailError::BadResponse(Redacted::new("the token answer is incomplete")))?;
        if raw.access_token.is_empty() {
            return Err(MailError::BadResponse(Redacted::new(
                "the token answer carries an empty access token",
            )));
        }
        let refresh_token = raw
            .refresh_token
            .or_else(|| fallback_refresh.map(str::to_owned))
            .map(Secret::new);
        Ok(Self {
            access_token: Secret::new(raw.access_token),
            refresh_token,
            expires_in: raw.expires_in,
        })
    }
}

#[derive(Deserialize)]
struct RawTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Deserialize)]
struct RawDeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    DEFAULT_INTERVAL_SECS
}

/// Ask Microsoft for a device code. The returned code is shown to the user by
/// the caller (MAIL1C renders the card); the device code stays in memory.
pub async fn start_device_login(
    transport: &dyn HttpTransport,
    client_id: &str,
    label: &str,
    tenant: Tenant,
) -> Result<DeviceLogin, MailError> {
    assert_scopes_safe()?;
    device_code(transport, client_id, label, tenant, &SCOPES).await
}

/// The send consent (MAIL2A): a device login that asks for `Mail.Send` only.
/// Its tokens are stored apart from the login ones (see `Account::send`).
pub async fn start_send_consent(
    transport: &dyn HttpTransport,
    client_id: &str,
    label: &str,
    tenant: Tenant,
) -> Result<DeviceLogin, MailError> {
    assert_scopes_safe()?;
    assert_send_scopes_safe()?;
    device_code(transport, client_id, label, tenant, &SEND_SCOPES).await
}

async fn device_code(
    transport: &dyn HttpTransport,
    client_id: &str,
    label: &str,
    tenant: Tenant,
    scopes: &[&str],
) -> Result<DeviceLogin, MailError> {
    let scope = scopes.join(" ");
    let form = [("client_id", client_id), ("scope", scope.as_str())];
    let response = transport
        .post_form(&endpoint(tenant, "oauth2/v2.0/devicecode"), &form)
        .await?;
    let body = response.json()?;
    if !response.is_success() {
        return Err(aad_error(&body));
    }
    let raw: RawDeviceCode = serde_json::from_value(body).map_err(|_| {
        MailError::BadResponse(Redacted::new("the device-code answer is incomplete"))
    })?;
    Ok(DeviceLogin::new(
        label.to_owned(),
        raw.user_code,
        raw.verification_uri,
        raw.expires_in,
        raw.interval.max(1),
        Secret::new(raw.device_code),
    ))
}

/// Poll the token endpoint until the user finishes, respecting `interval` and
/// adding five seconds whenever Microsoft answers `slow_down`. `expired_token`
/// and `access_denied` are terminal; the loop also stops at the code's own
/// lifetime, so a silent server cannot keep it alive forever.
pub async fn poll_device_login(
    transport: &dyn HttpTransport,
    client_id: &str,
    tenant: Tenant,
    login: &DeviceLogin,
) -> Result<Tokens, MailError> {
    assert_scopes_safe()?;
    let url = endpoint(tenant, "oauth2/v2.0/token");
    let mut interval = login.interval.max(1);
    let lifetime = login.expires_in.clamp(1, MAX_LOGIN_SECS);
    let deadline = Instant::now() + Duration::from_secs(lifetime);

    loop {
        let form = [
            ("grant_type", DEVICE_CODE_GRANT),
            ("client_id", client_id),
            ("device_code", login.device_code.expose()),
        ];
        let response = transport.post_form(&url, &form).await?;
        let body = response.json()?;
        if response.is_success() {
            return Tokens::from_body(&body, None);
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval = interval.saturating_add(SLOW_DOWN_STEP_SECS),
            Some("expired_token") => return Err(MailError::LoginExpired),
            Some("access_denied") => return Err(MailError::LoginDenied),
            _ => return Err(aad_error(&body)),
        }
        let wait = Duration::from_secs(interval);
        if Instant::now() + wait > deadline {
            return Err(MailError::LoginExpired);
        }
        sleep(wait).await;
    }
}

/// Exchange a refresh token. When Microsoft does not rotate it, the presented
/// one is kept, so the account never loses its only refresh token.
pub async fn refresh(
    transport: &dyn HttpTransport,
    client_id: &str,
    tenant: Tenant,
    refresh_token: &str,
) -> Result<Tokens, MailError> {
    assert_scopes_safe()?;
    exchange_refresh(transport, client_id, tenant, refresh_token, &SCOPES).await
}

/// The access token for sending (MAIL2A). Its refresh token is the send
/// consent's, so it can never be spent on a login-scope request.
pub async fn refresh_send(
    transport: &dyn HttpTransport,
    client_id: &str,
    tenant: Tenant,
    refresh_token: &str,
) -> Result<Tokens, MailError> {
    assert_scopes_safe()?;
    assert_send_scopes_safe()?;
    exchange_refresh(transport, client_id, tenant, refresh_token, &SEND_SCOPES).await
}

async fn exchange_refresh(
    transport: &dyn HttpTransport,
    client_id: &str,
    tenant: Tenant,
    refresh_token: &str,
    scopes: &[&str],
) -> Result<Tokens, MailError> {
    let scope = scopes.join(" ");
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("scope", scope.as_str()),
    ];
    let response = transport
        .post_form(&endpoint(tenant, "oauth2/v2.0/token"), &form)
        .await?;
    let body = response.json()?;
    if !response.is_success() {
        return Err(aad_error(&body));
    }
    Tokens::from_body(&body, Some(refresh_token))
}

/// Turn an Entra ID refusal into a typed error. The description is kept only
/// through [`Redacted`], so an `AADSTS` message that echoes a token is safe.
fn aad_error(body: &Value) -> MailError {
    let code = body
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown_error");
    let description = body
        .get("error_description")
        .and_then(Value::as_str)
        .unwrap_or("Microsoft did not explain the refusal");
    let detail = Redacted::new(description);
    match aadsts(description) {
        Some("AADSTS65001" | "AADSTS90094") => MailError::AdminConsentRequired { detail },
        Some("AADSTS700016") => MailError::AppNotRegistered { detail },
        _ => MailError::AuthRejected {
            code: code.to_owned(),
            message: detail,
        },
    }
}

fn aadsts(description: &str) -> Option<&str> {
    description
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find(|part| part.starts_with("AADSTS"))
}
