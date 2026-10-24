//! Signed `/v1/chat` calls. Every method returns the reduced contract model;
//! page cursors stay explicit so history is never guessed.

use std::time::Duration;

use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Map, Value};

use super::error::{decode, protocol, ChatFailure, ChatResult};
use super::model::*;
use super::{validate_client_id, validate_prefixed_id, ConversationPatch};
use crate::client::GatewayClient;
use crate::error::CoreError;

const CHAT_TIMEOUT: Duration = Duration::from_secs(20);
/// One page of chat history. Reconnection follows the cursor until a short
/// page, so a long offline gap does not lose the middle of the thread.
pub const CHAT_PAGE_LIMIT: u16 = 60;

impl GatewayClient {
    pub async fn chat_bootstrap(&self) -> ChatResult<ChatBootstrap> {
        self.chat_get("/v1/chat/bootstrap", &[]).await
    }

    pub async fn chat_conversations(
        &self,
        q: Option<&str>,
        section_id: Option<&str>,
        pinned: bool,
        archived: bool,
    ) -> ChatResult<ConversationPage> {
        if let Some(section_id) = section_id {
            validate_prefixed_id(section_id, "sec_", "sección")?;
        }
        let mut query = vec![("limit", CHAT_PAGE_LIMIT.to_string())];
        if let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) {
            query.push(("q", q.to_string()));
        }
        if let Some(section_id) = section_id {
            query.push(("section_id", section_id.to_string()));
        }
        if pinned {
            query.push(("pinned", "true".to_string()));
        }
        if archived {
            query.push(("archived", "true".to_string()));
        }
        self.chat_get("/v1/chat/conversations", &query).await
    }

    pub async fn chat_create_conversation(&self, title: Option<&str>) -> ChatResult<Conversation> {
        let mut body = Map::new();
        if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) {
            body.insert("title".to_string(), Value::String(title.to_string()));
        }
        let created: CreatedConversation = self
            .chat_send_json(
                Method::POST,
                "/v1/chat/conversations",
                &[],
                &serde_json::to_value(body).map_err(protocol)?,
                true,
            )
            .await?;
        Ok(created.conversation)
    }

    pub async fn chat_conversation(&self, id: &str) -> ChatResult<ConversationDetail> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        self.chat_get(&format!("/v1/chat/conversations/{id}"), &[])
            .await
    }

    pub async fn chat_update_conversation(
        &self,
        id: &str,
        patch: &ConversationPatch,
    ) -> ChatResult<Conversation> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let bytes = patch.to_body()?;
        let body = serde_json::from_slice(&bytes).map_err(protocol)?;
        let updated: CreatedConversation = self
            .chat_send_json(
                Method::POST,
                &format!("/v1/chat/conversations/{id}/update"),
                &[],
                &body,
                true,
            )
            .await?;
        Ok(updated.conversation)
    }

    pub async fn chat_timeline(
        &self,
        id: &str,
        before_seq: Option<i64>,
    ) -> ChatResult<TimelinePage> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let mut query = vec![("limit", CHAT_PAGE_LIMIT.to_string())];
        if let Some(before) = before_seq {
            query.push(("before_seq", before.to_string()));
        }
        self.chat_get(&format!("/v1/chat/conversations/{id}/timeline"), &query)
            .await
    }

    pub async fn chat_messages(
        &self,
        id: &str,
        after_seq: Option<i64>,
        before_seq: Option<i64>,
    ) -> ChatResult<MessagePage> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let mut query = vec![("limit", CHAT_PAGE_LIMIT.to_string())];
        if let Some(after) = after_seq {
            query.push(("after_seq", after.to_string()));
        }
        if let Some(before) = before_seq {
            query.push(("before_seq", before.to_string()));
        }
        self.chat_get(&format!("/v1/chat/conversations/{id}/messages"), &query)
            .await
    }

    pub async fn chat_send(
        &self,
        id: &str,
        client_id: &str,
        text: &str,
    ) -> ChatResult<SendAccepted> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        validate_client_id(client_id)?;
        let text = text.trim();
        if text.is_empty() {
            return Err(ChatFailure::Invalid {
                message: "El mensaje está vacío.".to_string(),
                field: Some("text".to_string()),
            });
        }
        let body = json!({ "client_id": client_id, "text": text });
        self.chat_send_json(
            Method::POST,
            &format!("/v1/chat/conversations/{id}/messages"),
            &[],
            &body,
            true,
        )
        .await
    }

    pub async fn chat_read(&self, id: &str, last_seq: i64) -> ChatResult<ReadAccepted> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let body = json!({ "last_seq": last_seq.max(0) });
        self.chat_send_json(
            Method::POST,
            &format!("/v1/chat/conversations/{id}/read"),
            &[],
            &body,
            true,
        )
        .await
    }

    pub async fn chat_sections(&self) -> ChatResult<Vec<Section>> {
        let page: SectionPage = self.chat_get("/v1/chat/sections", &[]).await?;
        Ok(page.sections)
    }

    pub async fn chat_save_section(&self, id: Option<&str>, name: &str) -> ChatResult<Section> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ChatFailure::Invalid {
                message: "La sección necesita un nombre.".to_string(),
                field: Some("name".to_string()),
            });
        }
        let mut body = Map::new();
        body.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(id) = id {
            validate_prefixed_id(id, "sec_", "sección")?;
            body.insert("id".to_string(), Value::String(id.to_string()));
        }
        let saved: SectionSaved = self
            .chat_send_json(
                Method::POST,
                "/v1/chat/sections",
                &[],
                &Value::Object(body),
                true,
            )
            .await?;
        Ok(saved.section)
    }

    pub(super) async fn chat_get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> ChatResult<T> {
        self.chat_call(Method::GET, path, query, None, false).await
    }

    pub(super) async fn chat_send_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: &Value,
        write: bool,
    ) -> ChatResult<T> {
        let bytes = serde_json::to_vec(body).map_err(protocol)?;
        self.chat_call(method, path, query, Some(&bytes), write)
            .await
    }

    async fn chat_call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&[u8]>,
        write: bool,
    ) -> ChatResult<T> {
        let request = self
            .signed_call(method, path, query, body)
            .map_err(|error| ChatFailure::from_core(&error, write))?;
        let response = request
            .timeout(CHAT_TIMEOUT)
            .send()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), write))?;
        decode(response, write).await
    }
}

#[derive(Debug, Deserialize)]
struct CreatedConversation {
    conversation: Conversation,
}

#[derive(Debug, Deserialize)]
struct SectionSaved {
    section: Section,
}
