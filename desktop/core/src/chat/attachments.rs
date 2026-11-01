//! Image attachments: the `…/attachments` routes that stage a file before a
//! message takes it. The host checks, decodes and re-encodes the bytes; this
//! side moves them in base64 chunks and keeps every coded refusal.

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error::{status_failure, ChatFailure, ChatResult};
use super::{validate_client_id, validate_prefixed_id};
use crate::client::GatewayClient;
use crate::error::CoreError;

/// The host stores every ready file re-encoded as WebP.
const CONTENT_MIME: &str = "image/webp";

/// Refusals only the attachment routes answer. `invalid_name` and
/// `invalid_body` are left out: the team and the chat answer them too.
const REFUSALS: &[&str] = &[
    "unknown_attachment",
    "too_many_attachments",
    "no_such_attachment",
    "not_uploading",
    "offset_mismatch",
    "not_ready",
    "already_linked",
    "file_too_large",
    "chunk_too_large",
    "conversation_quota",
    "host_quota",
    "invalid_sha256",
    "invalid_base64",
    "size_mismatch",
    "hash_mismatch",
    "unsupported_type",
    "invalid_image",
    "image_too_large",
    "dimensions_too_large",
    "input_too_large",
    "timeout",
];

/// One attachment as the routes and the timeline describe it. Fields stay
/// `None` while the upload is still in progress.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    pub name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<u64>,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub state: Option<String>,
}

/// The stored bytes of a ready attachment, base64 so they cross IPC as text.
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentContent {
    pub mime: &'static str,
    pub data: String,
}

#[derive(Deserialize)]
struct Envelope {
    attachment: Attachment,
}

#[derive(Deserialize)]
struct Chunked {
    next_offset: u64,
}

/// The coded refusal of an attachment route, or `None` when the body is not one.
pub(super) fn attachment_refusal(body: &[u8]) -> Option<ChatFailure> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let (code, field, next_offset) = match value.get("detail")? {
        Value::String(code) => (code.as_str(), None, None),
        Value::Object(fields) => (
            fields.get("code")?.as_str()?,
            fields.get("field").and_then(Value::as_str),
            fields.get("next_offset").and_then(Value::as_u64),
        ),
        _ => return None,
    };
    REFUSALS
        .contains(&code)
        .then(|| ChatFailure::AttachmentRefused {
            code: code.to_string(),
            field: field.map(str::to_string),
            next_offset,
        })
}

fn attachment_path(conversation_id: &str, attachment_id: &str) -> ChatResult<String> {
    validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
    validate_prefixed_id(attachment_id, "att_", "adjunto")?;
    Ok(format!(
        "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}"
    ))
}

impl GatewayClient {
    /// Starts an upload. A repeat with the same `client_id` answers the first attachment.
    pub async fn chat_attachment_create(
        &self,
        conversation_id: &str,
        client_id: &str,
        name: &str,
        size: u64,
        sha256: &str,
    ) -> ChatResult<Attachment> {
        validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
        validate_client_id(client_id)?;
        let body = json!({ "client_id": client_id, "name": name, "size": size, "sha256": sha256 });
        let path = format!("/v1/chat/conversations/{conversation_id}/attachments");
        let created: Envelope = self
            .chat_send_json(Method::POST, &path, &[], &body, true)
            .await?;
        Ok(created.attachment)
    }

    /// Appends the chunk at `offset`; returns where the next one starts.
    pub async fn chat_attachment_chunk(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        offset: u64,
        data: &str,
    ) -> ChatResult<u64> {
        let path = attachment_path(conversation_id, attachment_id)?;
        let body = json!({ "offset": offset, "data": data });
        let chunked: Chunked = self
            .chat_send_json(Method::POST, &format!("{path}/chunks"), &[], &body, true)
            .await?;
        Ok(chunked.next_offset)
    }

    pub async fn chat_attachment_complete(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> ChatResult<Attachment> {
        let path = attachment_path(conversation_id, attachment_id)?;
        let completed: Envelope = self
            .chat_call(Method::POST, &format!("{path}/complete"), &[], None, true)
            .await?;
        Ok(completed.attachment)
    }

    /// Drops an upload in progress or a ready file that no message has taken.
    pub async fn chat_attachment_delete(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> ChatResult<()> {
        let path = attachment_path(conversation_id, attachment_id)?;
        let _: Value = self
            .chat_call(Method::DELETE, &path, &[], None, true)
            .await?;
        Ok(())
    }

    /// The bytes are not JSON, so this call reads the response itself.
    pub async fn chat_attachment_content(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> ChatResult<AttachmentContent> {
        let path = attachment_path(conversation_id, attachment_id)?;
        let request = self
            .signed_call(Method::GET, &format!("{path}/content"), &[], None)
            .map_err(|error| ChatFailure::from_core(&error, false))?;
        let response = request
            .send()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), false))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), false))?;
        if !status.is_success() {
            return Err(status_failure(status, &bytes, None, false));
        }
        Ok(AttachmentContent {
            mime: CONTENT_MIME,
            data: STANDARD.encode(&bytes),
        })
    }
}
