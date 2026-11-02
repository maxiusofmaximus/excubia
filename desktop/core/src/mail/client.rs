//! Signed `/v1/mail` calls. Every method answers the reduced model, and a refusal
//! is a `MailFailure` with its contract code, never the raw body. Ids and labels
//! are checked before they reach the path or the query.

use std::time::Duration;

use reqwest::{Method, Response};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use super::{
    failure_from_status, DeviceLogin, LoginStatus, MailAccount, MailDraft, MailFailure, MailResult,
    SendRequest, SendState, SendToggled,
};
use crate::client::HostClient;

const MAIL_TIMEOUT: Duration = Duration::from_secs(20);
const TENANTS: [&str; 2] = ["consumers", "organizations"];

impl HostClient {
    pub async fn mail_accounts(&self) -> MailResult<Vec<MailAccount>> {
        let page: AccountPage = self
            .mail_call(Method::GET, "/v1/mail/accounts", &[], None)
            .await?;
        Ok(page.accounts)
    }

    /// Starts the device-code sign-in of a new account. The code is for the
    /// operator to type at `verification_uri`; nothing is asked of him here.
    pub async fn mail_login_start(&self, account: &str, tenant: &str) -> MailResult<DeviceLogin> {
        validate_label(account)?;
        if !TENANTS.contains(&tenant) {
            return Err(MailFailure::coded("mail_invalid_input", None));
        }
        let body = json!({ "account": account, "tenant": tenant });
        self.mail_call(Method::POST, "/v1/mail/login/start", &[], Some(&body))
            .await
    }

    pub async fn mail_login_status(&self, account: &str) -> MailResult<LoginStatus> {
        validate_label(account)?;
        let page: LoginPage = self
            .mail_call(
                Method::GET,
                "/v1/mail/login/status",
                &[("account", account.to_string())],
                None,
            )
            .await?;
        Ok(page.login.localized())
    }

    /// Starts the second sign-in that grants `Mail.Send`. The same code flow as a
    /// normal sign-in; the consent is kept apart from the read one.
    pub async fn mail_send_enable(&self, account: &str) -> MailResult<DeviceLogin> {
        validate_label(account)?;
        let body = json!({ "account": account });
        self.mail_call(Method::POST, "/v1/mail/send/enable", &[], Some(&body))
            .await
    }

    pub async fn mail_send_enable_status(&self, account: &str) -> MailResult<LoginStatus> {
        validate_label(account)?;
        let page: LoginPage = self
            .mail_call(
                Method::GET,
                "/v1/mail/send/enable/status",
                &[("account", account.to_string())],
                None,
            )
            .await?;
        Ok(page.login.localized())
    }

    pub async fn mail_send_disable(&self, account: &str) -> MailResult<SendToggled> {
        validate_label(account)?;
        let body = json!({ "account": account });
        self.mail_call(Method::POST, "/v1/mail/send/disable", &[], Some(&body))
            .await
    }

    /// The whole draft, body included: the only route that returns it. It opens
    /// no question and changes nothing.
    pub async fn mail_draft(&self, account: &str, draft_id: &str) -> MailResult<MailDraft> {
        validate_label(account)?;
        validate_segment(draft_id)?;
        let page: DraftPage = self
            .mail_call(
                Method::GET,
                &format!("/v1/mail/drafts/{draft_id}"),
                &[("account", account.to_string())],
                None,
            )
            .await?;
        Ok(page.draft)
    }

    /// Saves the operator's edit of the subject and the body. The host closes a
    /// send open on the draft; the answer is the draft as it now is.
    pub async fn mail_draft_update(
        &self,
        account: &str,
        draft_id: &str,
        subject: &str,
        body: &str,
    ) -> MailResult<MailDraft> {
        validate_label(account)?;
        validate_segment(draft_id)?;
        let request = json!({ "account": account, "subject": subject, "body": body });
        let page: DraftPage = self
            .mail_call(
                Method::POST,
                &format!("/v1/mail/drafts/{draft_id}/update"),
                &[],
                Some(&request),
            )
            .await?;
        Ok(page.draft)
    }

    /// Without an approval, asks for one. With the approval the operator gave in
    /// the chat, schedules the send and starts its cancellation window.
    pub async fn mail_draft_send(
        &self,
        account: &str,
        draft_id: &str,
        approval_id: Option<&str>,
    ) -> MailResult<SendRequest> {
        validate_label(account)?;
        validate_segment(draft_id)?;
        let mut body = json!({ "account": account });
        if let Some(approval_id) = approval_id {
            validate_segment(approval_id)?;
            body["approval_id"] = Value::String(approval_id.to_string());
        }
        self.mail_call(
            Method::POST,
            &format!("/v1/mail/drafts/{draft_id}/send"),
            &[],
            Some(&body),
        )
        .await
    }

    pub async fn mail_send_state(&self, send_id: &str) -> MailResult<SendState> {
        validate_segment(send_id)?;
        self.mail_call(Method::GET, &format!("/v1/mail/sends/{send_id}"), &[], None)
            .await
    }

    /// Cancels inside the window. The answer is the state the host settled on:
    /// `cancelled`, or `sent` when the window had already closed.
    pub async fn mail_send_cancel(&self, send_id: &str) -> MailResult<SendState> {
        validate_segment(send_id)?;
        self.mail_call(
            Method::POST,
            &format!("/v1/mail/sends/{send_id}/cancel"),
            &[],
            Some(&json!({})),
        )
        .await
    }

    async fn mail_call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> MailResult<T> {
        let bytes = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| MailFailure::coded("mail_bad_response", None))?;
        let request = self
            .signed_call(method, path, query, bytes.as_deref())
            .map_err(|_| MailFailure::coded("mail_unavailable", None))?;
        let response = request
            .timeout(MAIL_TIMEOUT)
            .send()
            .await
            .map_err(|_| MailFailure::coded("mail_unavailable", None))?;
        decode(response).await
    }
}

async fn decode<T: DeserializeOwned>(response: Response) -> MailResult<T> {
    let status = response.status();
    let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let bytes = response
        .bytes()
        .await
        .map_err(|_| MailFailure::coded("mail_unavailable", None))?;
    if !status.is_success() {
        return Err(failure_from_status(status, &bytes, retry_after_secs));
    }
    serde_json::from_slice(&bytes).map_err(|_| MailFailure::coded("mail_bad_response", None))
}

/// An account label: what the operator named it, sent in a query or a body.
fn validate_label(label: &str) -> MailResult<()> {
    let valid = !label.trim().is_empty()
        && label.chars().count() <= 64
        && !label.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(MailFailure::coded("mail_invalid_input", None))
    }
}

/// An id that goes into a path. The host's ids are short and plain; anything
/// else (a slash, a dot, a query mark, a space) never leaves the client.
pub(crate) fn validate_segment(id: &str) -> MailResult<()> {
    let valid = (1..=128).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(MailFailure::coded("mail_invalid_input", None))
    }
}

#[derive(Debug, Deserialize)]
struct AccountPage {
    accounts: Vec<MailAccount>,
}

#[derive(Debug, Deserialize)]
struct LoginPage {
    login: LoginStatus,
}

#[derive(Debug, Deserialize)]
struct DraftPage {
    draft: MailDraft,
}
