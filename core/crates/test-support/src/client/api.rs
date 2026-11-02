//! The authenticated endpoints of the host, one method per route.

use serde_json::{json, Value};

use super::signed::{signed_query, HttpError, SignedClient};

impl SignedClient {
    pub async fn state(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/state", None).await
    }

    pub async fn respond(&self, pane_id: &str, text: &str) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "text": text })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/respond"), Some(body))
            .await
    }

    pub async fn keys(
        &self,
        pane_id: &str,
        keys: &[&str],
        confirm: bool,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "keys": keys, "confirm": confirm })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/keys"), Some(body))
            .await
    }

    pub async fn read(&self, pane_id: &str, lines: u16) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({ "lines": lines })).expect("json");
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/read"), Some(body))
            .await
    }

    pub async fn seen(&self, pane_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/seen"), None)
            .await
    }

    /// Start a conversation from the host's agent catalogue.
    pub async fn create_workspace(
        &self,
        agent: &str,
        cwd: &str,
        label: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "agent": agent,
            "cwd": cwd,
            "label": label,
        }))
        .expect("json");
        self.expect_ok("POST", "/v1/workspaces", Some(body)).await
    }

    /// The host's agent catalogue: names and availability only.
    pub async fn catalog(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/catalog", None).await
    }

    /// Stop a live agent; the pane leaves the state list.
    pub async fn stop(&self, pane_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/agents/{pane_id}/stop"), None)
            .await
    }

    pub async fn chat_bootstrap(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/bootstrap", None).await
    }

    pub async fn chat_conversations(&self, params: &[(&str, &str)]) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!("/v1/chat/conversations{}", signed_query(params)),
            None,
        )
        .await
    }

    pub async fn chat_counts(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/conversations/counts", None)
            .await
    }

    pub async fn chat_create(
        &self,
        title: &str,
        project_id: Option<&str>,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "title": title,
            "project_id": project_id,
        }))
        .expect("json");
        self.expect_ok("POST", "/v1/chat/conversations", Some(body))
            .await
    }

    pub async fn chat_conversation(&self, conversation_id: &str) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!("/v1/chat/conversations/{conversation_id}"),
            None,
        )
        .await
    }

    pub async fn chat_update(
        &self,
        conversation_id: &str,
        body: Value,
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/update"),
            Some(serde_json::to_vec(&body).expect("json")),
        )
        .await
    }

    pub async fn chat_timeline(
        &self,
        conversation_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!(
                "/v1/chat/conversations/{conversation_id}/timeline{}",
                signed_query(params)
            ),
            None,
        )
        .await
    }

    pub async fn chat_messages(
        &self,
        conversation_id: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, HttpError> {
        self.expect_ok(
            "GET",
            &format!(
                "/v1/chat/conversations/{conversation_id}/messages{}",
                signed_query(params)
            ),
            None,
        )
        .await
    }

    pub async fn chat_send(
        &self,
        conversation_id: &str,
        client_id: &str,
        text: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({
            "client_id": client_id,
            "text": text,
        }))
        .expect("json");
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/messages"),
            Some(body),
        )
        .await
    }

    pub async fn chat_read(
        &self,
        conversation_id: &str,
        last_seq: i64,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({"last_seq": last_seq})).expect("json");
        self.expect_ok(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/read"),
            Some(body),
        )
        .await
    }

    pub async fn chat_sections(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/chat/sections", None).await
    }

    pub async fn chat_upsert_section(
        &self,
        id: Option<&str>,
        name: &str,
    ) -> Result<Value, HttpError> {
        let body = serde_json::to_vec(&json!({"id": id, "name": name})).expect("json");
        self.expect_ok("POST", "/v1/chat/sections", Some(body))
            .await
    }

    pub async fn devices(&self) -> Result<Value, HttpError> {
        self.expect_ok("GET", "/v1/devices", None).await
    }

    pub async fn revoke(&self, device_id: &str) -> Result<Value, HttpError> {
        self.expect_ok("POST", &format!("/v1/devices/{device_id}/revoke"), None)
            .await
    }
}
