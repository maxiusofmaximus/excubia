//! Image attachments: the `…/attachments` routes that stage a file before a
//! message takes it. The host checks, decodes and re-encodes the bytes; this
//! side moves them in base64 chunks and keeps every coded refusal.

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{header::CONTENT_DISPOSITION, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error::{status_failure, ChatFailure, ChatResult};
use super::{validate_client_id, validate_prefixed_id};
use crate::client::HostClient;
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

/// What the host keeps: a picture, re-encoded to WebP, or a document as it was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Document,
}

/// One attachment as the routes and the timeline describe it. Fields stay
/// `None` while the upload is still in progress. A picture has no `kind`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    pub name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<u64>,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub state: Option<String>,
    #[serde(default)]
    pub kind: Option<AttachmentKind>,
}

/// The stored bytes of a ready attachment, base64 so they cross IPC as text.
#[derive(Debug, Clone, Serialize)]
pub struct AttachmentContent {
    pub mime: &'static str,
    pub data: String,
}

/// The bytes of a ready attachment as the host keeps them, with the name its
/// download carries (`None` when the host sends no name).
#[derive(Debug, Clone)]
pub struct AttachmentFile {
    pub name: Option<String>,
    pub bytes: Vec<u8>,
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

pub(super) fn attachment_path(conversation_id: &str, attachment_id: &str) -> ChatResult<String> {
    validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
    validate_prefixed_id(attachment_id, "att_", "adjunto")?;
    Ok(format!(
        "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}"
    ))
}

impl HostClient {
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
        let file = self
            .chat_attachment_file(conversation_id, attachment_id)
            .await?;
        Ok(AttachmentContent {
            mime: CONTENT_MIME,
            data: STANDARD.encode(&file.bytes),
        })
    }

    /// The stored bytes as they were sent, with the name the download gives.
    pub async fn chat_attachment_file(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> ChatResult<AttachmentFile> {
        let path = attachment_path(conversation_id, attachment_id)?;
        let request = self
            .signed_call(Method::GET, &format!("{path}/content"), &[], None)
            .map_err(|error| ChatFailure::from_core(&error, false))?;
        let response = request
            .send()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), false))?;
        let status = response.status();
        let name = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(filename_from_disposition);
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), false))?;
        if !status.is_success() {
            return Err(status_failure(status, &bytes, None, false));
        }
        Ok(AttachmentFile {
            name,
            bytes: bytes.into(),
        })
    }
}

/// The file name of a `Content-Disposition` header: the RFC 5987 `filename*`
/// when the host sends one, otherwise a plain `filename`.
pub fn filename_from_disposition(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';').map(str::trim) {
        if let Some(name) = part.strip_prefix("filename*=").and_then(rfc5987_name) {
            return Some(name);
        }
        if let Some(quoted) = part.strip_prefix("filename=") {
            plain = Some(quoted.trim_matches('"').to_string()).filter(|name| !name.is_empty());
        }
    }
    plain
}

fn rfc5987_name(value: &str) -> Option<String> {
    let encoded = value
        .strip_prefix("UTF-8''")
        .or_else(|| value.strip_prefix("utf-8''"))?;
    percent_decode(encoded).filter(|name| !name.is_empty())
}

fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.bytes();
    while let Some(byte) = rest.next() {
        if byte != b'%' {
            bytes.push(byte);
            continue;
        }
        let high = hex_digit(rest.next()?)?;
        let low = hex_digit(rest.next()?)?;
        bytes.push(high * 16 + low);
    }
    String::from_utf8(bytes).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

#[cfg(test)]
mod tests {
    use super::{filename_from_disposition, percent_decode};

    #[test]
    fn the_rfc_5987_name_wins_and_is_decoded() {
        let header =
            "attachment; filename=\"plain.xlsx\"; filename*=UTF-8''Informe%20Q1%20%C3%B1.xlsx";
        assert_eq!(
            filename_from_disposition(header).as_deref(),
            Some("Informe Q1 ñ.xlsx")
        );
    }

    #[test]
    fn a_plain_name_is_read_without_its_quotes() {
        assert_eq!(
            filename_from_disposition("attachment; filename=\"bitacora.pdf\"").as_deref(),
            Some("bitacora.pdf")
        );
    }

    #[test]
    fn a_missing_or_broken_name_is_none() {
        assert_eq!(filename_from_disposition("attachment"), None);
        assert_eq!(filename_from_disposition("attachment; filename=\"\""), None);
        assert_eq!(
            filename_from_disposition("attachment; filename*=UTF-8''%zz"),
            None
        );
        assert_eq!(percent_decode("%C3"), None, "a cut escape is not UTF-8");
    }
}
