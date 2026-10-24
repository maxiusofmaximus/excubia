//! The approval surface of the signed test client.

use crate::client::{signed_query, HttpError, SignedClient};
use serde_json::{json, Value};

impl SignedClient {
    /// The UAC inbox.
    pub async fn chat_approvals(&self, params: &[(&str, &str)]) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!("/v1/chat/approvals{}", signed_query(params)),
            None,
        )
        .await
    }

    /// The operator's answer. The same `client_id` always gets the same one.
    pub async fn chat_decide_approval(
        &self,
        approval_id: &str,
        decision: &str,
        client_id: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "decision": decision,
            "scope": "once",
            "client_id": client_id,
        }))
        .expect("json");
        self.expect_ok(
            "POST",
            &format!("/v1/chat/approvals/{approval_id}/decision"),
            Some(body),
        )
        .await
    }

    /// The approval modes: the global one and the project overrides.
    pub async fn chat_approval_settings(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/approvals/settings", None)
            .await
    }

    /// Set the global mode, or the mode of one project when `project_id` is
    /// given. The same `client_id` changes nothing twice.
    pub async fn chat_set_approval_mode(
        &self,
        client_id: &str,
        mode: &str,
        project_id: Option<&str>,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "client_id": client_id,
            "mode": mode,
            "project_id": project_id,
        }))
        .expect("json");
        self.expect_ok("POST", "/v1/chat/approvals/settings", Some(body))
            .await
    }
}
