//! The operator's GitHub link card (GITHUB-INTEGRACION). The card reads the state of the
//! link the operator approved and may cancel it. Both calls are signed and need `APPROVE`
//! on the host. The code travels only in the answer of the read, shown in memory by the
//! card: it is never stored here, never logged and never sent back.

use std::fmt;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::error::ChatResult;
use super::validate_prefixed_id;
use crate::client::HostClient;

/// The link's state as the card shows it. `code` is present only while the login shows it.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GithubLinkCard {
    pub phase: String,
    #[serde(default)]
    pub code: Option<String>,
    pub url: String,
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub failure: Option<String>,
    /// The questions this attempt serves: a card shows the code only for its own question.
    #[serde(default)]
    pub approval_ids: Vec<String>,
}

// The code is a one-time login secret: a debug print must never show it.
impl fmt::Debug for GithubLinkCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubLinkCard")
            .field("phase", &self.phase)
            .field("code", &self.code.as_ref().map(|_| "<hidden>"))
            .field("url", &self.url)
            .field("login", &self.login)
            .field("failure", &self.failure)
            .field("approval_ids", &self.approval_ids)
            .finish()
    }
}

#[derive(Deserialize)]
struct LinkState {
    #[serde(default)]
    link: Option<GithubLinkCard>,
}

#[derive(Deserialize)]
struct LinkCancelled {
    cancelled: bool,
}

impl HostClient {
    /// The card's state, or `None` while no link was approved.
    pub async fn chat_github_link(&self) -> ChatResult<Option<GithubLinkCard>> {
        let state: LinkState = self
            .chat_call(Method::GET, "/v1/github/link", &[], None, false)
            .await?;
        Ok(state.link)
    }

    /// Stops the link of one question. `false` when that question's link is not running.
    pub async fn chat_github_link_cancel(&self, approval_id: &str) -> ChatResult<bool> {
        validate_prefixed_id(approval_id, "apr_", "aprobación")?;
        let body = serde_json::to_vec(&json!({ "approval_id": approval_id }))
            .map_err(super::error::protocol)?;
        let done: LinkCancelled = self
            .chat_call(Method::POST, "/v1/github/link/cancel", &[], Some(&body), true)
            .await?;
        Ok(done.cancelled)
    }
}
