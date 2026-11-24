//! Signed `/v1/chat` calls. Every method returns the reduced contract model;
//! page cursors stay explicit so history is never guessed.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Map, Value};

use super::error::{decode, protocol, ChatFailure, ChatResult};
use super::model::*;
use super::{validate_client_id, validate_prefixed_id, ConversationPatch};
use crate::client::HostClient;
use crate::ui_lang::pick;
use crate::error::CoreError;

const CHAT_TIMEOUT: Duration = Duration::from_secs(20);
/// One page of chat history. Reconnection follows the cursor until a short
/// page, so a long offline gap does not lose the middle of the thread.
pub const CHAT_PAGE_LIMIT: u16 = 60;

impl HostClient {
    pub async fn chat_bootstrap(&self) -> ChatResult<ChatBootstrap> {
        self.chat_get("/v1/chat/bootstrap", &[]).await
    }

    pub async fn chat_conversations(
        &self,
        q: Option<&str>,
        section_id: Option<&str>,
        pinned: bool,
        archived: bool,
        filter: Option<ListFilter>,
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
        if let Some(filter) = filter {
            query.push((filter.param(), "1".to_string()));
        }
        self.chat_get("/v1/chat/conversations", &query).await
    }

    /// The chip numbers. A host without the route answers `None`: its chips stay hidden.
    pub async fn chat_counts(&self) -> ChatResult<Option<ChatCounts>> {
        match self.chat_get("/v1/chat/conversations/counts", &[]).await {
            Ok(counts) => Ok(Some(counts)),
            Err(failure) if counts_unsupported(&failure) => Ok(None),
            Err(failure) => Err(failure),
        }
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

    /// Deletes the conversation for good. The host refuses while work in it is open.
    pub async fn chat_delete_conversation(&self, id: &str) -> ChatResult<()> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let _: serde_json::Value = self
            .chat_call(
                Method::DELETE,
                &format!("/v1/chat/conversations/{id}"),
                &[],
                None,
                true,
            )
            .await?;
        Ok(())
    }

    /// Deletes the chosen conversations in one request, one write of the host's budget.
    /// A busy or missing id lands in `failed`; the rest still go.
    pub async fn chat_delete_conversations(&self, ids: &[String]) -> ChatResult<BulkDelete> {
        for id in ids {
            validate_prefixed_id(id, "cnv_", "conversación")?;
        }
        self.chat_send_json(
            Method::POST,
            "/v1/chat/conversations/delete",
            &[],
            &json!({ "ids": ids }),
            true,
        )
        .await
    }

    /// Whether `chat.db` is encrypted and whether its plain backup is still kept.
    pub async fn vault_status(&self) -> ChatResult<VaultStatus> {
        self.chat_get("/v1/vault/status", &[]).await
    }

    /// Creates the key from the mouse samples (packed 16-byte records, base64) and returns the
    /// phrase once. The samples are checked here so a bad pack never reaches the host.
    pub async fn vault_init(&self, samples_b64: &str) -> ChatResult<VaultInit> {
        vault_samples_check(samples_b64)?;
        self.chat_send_json(
            Method::POST,
            "/v1/vault/init",
            &[],
            &json!({ "samples": samples_b64 }),
            true,
        )
        .await
    }

    /// Removes the plain backup of the migration (the host overwrites it with zeros).
    pub async fn vault_finish(&self) -> ChatResult<VaultDone> {
        self.chat_call(Method::POST, "/v1/vault/finish", &[], None, true)
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
        self.chat_send_reply(id, client_id, text, None).await
    }

    /// `reply_to` is sent only when the operator is answering a message.
    pub async fn chat_send_reply(
        &self,
        id: &str,
        client_id: &str,
        text: &str,
        reply_to: Option<&str>,
    ) -> ChatResult<SendAccepted> {
        self.chat_send_message(id, client_id, text, reply_to, &[])
            .await
    }

    /// `attachments` are ready upload ids; with them the text may be empty.
    pub async fn chat_send_message(
        &self,
        id: &str,
        client_id: &str,
        text: &str,
        reply_to: Option<&str>,
        attachments: &[String],
    ) -> ChatResult<SendAccepted> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        validate_client_id(client_id)?;
        let text = text.trim();
        if text.is_empty() && attachments.is_empty() {
            return Err(ChatFailure::Invalid {
                message: pick("El mensaje está vacío.", "The message is empty.").to_string(),
                field: Some("text".to_string()),
            });
        }
        let mut body = json!({ "client_id": client_id, "text": text });
        if let Some(reply_to) = reply_to {
            validate_prefixed_id(reply_to, "msg_", "mensaje")?;
            body["reply_to"] = json!({ "kind": "message", "id": reply_to });
        }
        if !attachments.is_empty() {
            for attachment in attachments {
                validate_prefixed_id(attachment, "att_", "adjunto")?;
            }
            body["attachments"] = json!(attachments);
        }
        self.chat_send_json(
            Method::POST,
            &format!("/v1/chat/conversations/{id}/messages"),
            &[],
            &body,
            true,
        )
        .await
    }

    /// Stops the open turn of the conversation. Sent once per tap; the state
    /// of the turn comes back by `chat.message` frames, not by this answer.
    pub async fn chat_stop_conversation(&self, id: &str) -> ChatResult<StopOutcome> {
        validate_prefixed_id(id, "cnv_", "conversación")?;
        let path = format!("/v1/chat/conversations/{id}/stop");
        self.chat_call(Method::POST, &path, &[], None, true).await
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
                message: pick("La sección necesita un nombre.", "The section needs a name.").to_string(),
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

    pub(super) async fn chat_call<T: DeserializeOwned>(
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

/// An older host has no counts route: 404 is `NotFound`, 405 a host refusal.
fn counts_unsupported(failure: &ChatFailure) -> bool {
    matches!(
        failure,
        ChatFailure::NotFound { .. } | ChatFailure::Host { status: 405, .. }
    )
}

/// The packed samples the host takes: whole 16-byte records, at least 2 048 of them, and at most
/// 60 000 bytes (the screen keeps its pack under the host's 64 KiB route limit).
const VAULT_RECORD_BYTES: usize = 16;
const VAULT_MIN_BYTES: usize = 2048 * VAULT_RECORD_BYTES;
const VAULT_MAX_BYTES: usize = 60_000;

fn vault_samples_check(samples_b64: &str) -> ChatResult<()> {
    let refused = |code: &str| ChatFailure::VaultRefused {
        code: code.to_string(),
    };
    // Refuse before decoding: a longer text cannot be a legal pack.
    if samples_b64.len() > VAULT_MAX_BYTES / 3 * 4 + 4 {
        return Err(refused("vault_samples_invalid"));
    }
    let bytes = STANDARD
        .decode(samples_b64)
        .map_err(|_| refused("vault_samples_invalid"))?;
    if bytes.len() % VAULT_RECORD_BYTES != 0 || bytes.len() > VAULT_MAX_BYTES {
        return Err(refused("vault_samples_invalid"));
    }
    if bytes.len() < VAULT_MIN_BYTES {
        return Err(refused("vault_samples_insufficient"));
    }
    Ok(())
}

#[cfg(test)]
mod vault_tests {
    use super::*;

    fn refusal(result: ChatResult<()>) -> Option<String> {
        match result {
            Err(ChatFailure::VaultRefused { code }) => Some(code),
            _ => None,
        }
    }

    #[test]
    fn vault_samples_are_checked_before_they_leave() {
        let packed = |records: usize| STANDARD.encode(vec![0u8; records * VAULT_RECORD_BYTES]);
        assert_eq!(vault_samples_check(&packed(2048)), Ok(()));
        assert_eq!(vault_samples_check(&packed(3750)), Ok(()));
        assert_eq!(
            refusal(vault_samples_check(&packed(2047))).as_deref(),
            Some("vault_samples_insufficient")
        );
        assert_eq!(
            refusal(vault_samples_check(&packed(3751))).as_deref(),
            Some("vault_samples_invalid")
        );
        assert_eq!(
            refusal(vault_samples_check(
                &STANDARD.encode(vec![0u8; 2048 * 16 + 1])
            ))
            .as_deref(),
            Some("vault_samples_invalid")
        );
        assert_eq!(
            refusal(vault_samples_check("no es base64!")).as_deref(),
            Some("vault_samples_invalid")
        );
    }

    #[test]
    fn the_recovery_phrase_never_shows_in_debug() {
        let init = VaultInit {
            phrase: "alfa bravo charlie".to_string(),
        };
        let shown = format!("{init:?}");
        assert!(!shown.contains("alfa"), "{shown}");
    }
}
