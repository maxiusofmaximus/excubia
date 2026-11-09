//! Attachments over HTTP: a signed upload in chunks, the host's check and
//! re-encode on `complete` (see `finish`), and the read of the stored bytes.
//! The store owns the rows and the quotas, `AttachmentFiles` owns the bytes.

mod documents;
mod errors;
mod finish;
mod preview;

pub use preview::attachment_preview;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::fmt::Write;

use hn_core_chat::{Attachment, AttachmentError, AttachmentFiles, AttachmentState, FileKind};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

use super::support::{self, client_id};
use errors::map_error;

#[derive(Deserialize)]
struct InitBody {
    client_id: String,
    name: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct ChunkBody {
    offset: u64,
    data: String,
}

/// `POST …/attachments`: start an upload. Repeating a `client_id` answers with the first attachment.
pub async fn create_attachment(
    State(ctx): State<ApiContext>,
    Path(conversation_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: InitBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = client_id(&payload.client_id) {
        return error.into_response();
    }
    let started = ctx.chat.begin_attachment(
        &conversation_id,
        &payload.client_id,
        &payload.name,
        payload.size,
        &payload.sha256,
    );
    match started {
        Ok(attachment) => created(json!({"attachment": upload_view(&attachment)})),
        Err(error) => map_error(error).into_response(),
    }
}

/// `POST …/attachments/{att}/chunks`: append the next bytes of an upload.
pub async fn upload_chunk(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let payload: ChunkBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    let data = match STANDARD.decode(&payload.data) {
        Ok(data) => data,
        Err(_) => {
            return ApiError::invalid_field("invalid_base64", "data", "data must be base64")
                .into_response();
        }
    };
    match append_chunk(
        &ctx,
        &conversation_id,
        &attachment_id,
        payload.offset,
        &data,
    ) {
        Ok(next_offset) => support::ok_object(json!({"next_offset": next_offset})),
        Err(error) => error.into_response(),
    }
}

fn append_chunk(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
    offset: u64,
    data: &[u8],
) -> Result<u64, ApiError> {
    let attachment = owned_attachment(ctx, conversation_id, attachment_id)?;
    if attachment.state != AttachmentState::Uploading {
        return Err(map_error(AttachmentError::NotUploading));
    }
    // The quota counts the declared size; bytes past it would sit on disk unaccounted.
    if offset.saturating_add(data.len() as u64) > attachment.declared_size {
        return Err(map_error(AttachmentError::FileTooLarge));
    }
    file_store(ctx)?
        .append_chunk(conversation_id, attachment_id, offset, data)
        .map_err(map_error)
}

/// `POST …/attachments/{att}/complete`: check the declared size and hash, then check the document or process the image, and store it.
pub async fn complete_attachment(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
) -> Response {
    match finish::finish_upload(&ctx, &conversation_id, &attachment_id).await {
        Ok(attachment) => support::ok_object(json!({"attachment": meta_view(&attachment)})),
        Err(error) => error.into_response(),
    }
}

/// `GET …/attachments/{att}`: the metadata of an attachment of this conversation.
pub async fn show_attachment(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
) -> Response {
    match owned_attachment(&ctx, &conversation_id, &attachment_id) {
        Ok(attachment) => support::ok_object(json!({"attachment": meta_view(&attachment)})),
        Err(error) => error.into_response(),
    }
}

/// `GET …/attachments/{att}/content`: the stored bytes, typed by the host and never sniffed.
pub async fn attachment_content(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
) -> Response {
    match read_content(&ctx, &conversation_id, &attachment_id) {
        Ok((mime, name, bytes)) => served(&mime, &name, bytes),
        Err(error) => error.into_response(),
    }
}

fn read_content(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
) -> Result<(String, String, Vec<u8>), ApiError> {
    let attachment = owned_attachment(ctx, conversation_id, attachment_id)?;
    match (attachment.state, attachment.mime) {
        (AttachmentState::Ready, Some(mime)) => {
            let bytes = file_store(ctx)?
                .read_file(conversation_id, attachment_id)
                .map_err(map_error)?;
            Ok((mime, attachment.name, bytes))
        }
        _ => Err(map_error(AttachmentError::NotReady)),
    }
}

fn served(mime: &str, name: &str, bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CONTENT_DISPOSITION, content_disposition(name)),
            (header::CACHE_CONTROL, "private, no-store".to_owned()),
        ],
        bytes,
    )
        .into_response()
}

/// `attachment` with the user's name in RFC 5987 form, so the phone saves the
/// file under it. Path separators are dropped: the name never moves the save path.
fn content_disposition(name: &str) -> String {
    let mut encoded = String::from("attachment; filename*=UTF-8''");
    for byte in name.bytes().filter(|byte| !matches!(byte, b'/' | b'\\')) {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// `DELETE …/attachments/{att}`: drop an attachment that no message takes.
pub async fn delete_attachment(
    State(ctx): State<ApiContext>,
    Path((conversation_id, attachment_id)): Path<(String, String)>,
) -> Response {
    match remove_unlinked(&ctx, &conversation_id, &attachment_id) {
        Ok(()) => support::ok_object(json!({})),
        Err(error) => error.into_response(),
    }
}

fn remove_unlinked(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
) -> Result<(), ApiError> {
    owned_attachment(ctx, conversation_id, attachment_id)?;
    // The store drops only rows no message takes, so an empty answer means one was linked meanwhile.
    let dropped = ctx
        .chat
        .delete_unlinked(conversation_id, &[attachment_id])
        .map_err(map_error)?;
    if dropped.is_empty() {
        return Err(map_error(AttachmentError::AlreadyLinked));
    }
    file_store(ctx)?
        .remove(conversation_id, attachment_id)
        .map_err(map_error)
}

/// The attachment, if it belongs to this conversation. A foreign id gets the same 404 as an unknown one.
fn owned_attachment(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
) -> Result<Attachment, ApiError> {
    match ctx.chat.attachment(attachment_id) {
        Ok(attachment) if attachment.conversation_id == conversation_id => Ok(attachment),
        Ok(_) | Err(AttachmentError::NoSuchAttachment) => {
            Err(map_error(AttachmentError::UnknownAttachment))
        }
        Err(error) => Err(map_error(error)),
    }
}

fn file_store(ctx: &ApiContext) -> Result<AttachmentFiles, ApiError> {
    ctx.chat
        .attachment_files()
        .ok_or_else(|| ApiError::internal("the attachment folder is not configured"))
}

fn created(body: Value) -> Response {
    let mut response = support::ok_object(body);
    *response.status_mut() = StatusCode::CREATED;
    response
}

fn upload_view(attachment: &Attachment) -> Value {
    json!({"id": attachment.id, "state": state_label(attachment.state)})
}

fn meta_view(attachment: &Attachment) -> Value {
    json!({
        "id": attachment.id,
        "name": attachment.name,
        "mime": attachment.mime,
        "size": attachment.size,
        "w": attachment.width,
        "h": attachment.height,
        "kind": attachment.width.zip(attachment.height).map(|(w, h)| FileKind::of(w, h)),
        "previewable": attachment.previewable(),
        "state": state_label(attachment.state),
    })
}

fn state_label(state: AttachmentState) -> &'static str {
    match state {
        AttachmentState::Uploading => "uploading",
        AttachmentState::Ready => "ready",
    }
}

#[cfg(test)]
mod tests {
    use super::content_disposition;

    #[test]
    fn the_name_travels_percent_encoded_in_utf8() {
        assert_eq!(
            content_disposition("Informe Ñandú 2026.xlsx"),
            "attachment; filename*=UTF-8''Informe%20%C3%91and%C3%BA%202026.xlsx"
        );
    }

    #[test]
    fn path_separators_never_reach_the_header() {
        assert_eq!(
            content_disposition("..\\a/b.pdf"),
            "attachment; filename*=UTF-8''..ab.pdf"
        );
    }
}
