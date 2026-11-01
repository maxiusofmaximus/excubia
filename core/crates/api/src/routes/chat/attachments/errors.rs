//! The answer of each attachment refusal. Quotas and sizes are 413, the rest
//! follow `errors.md`: 404 for an unknown or foreign id, 409 for a state clash.

use axum::http::StatusCode;
use hn_core_chat::AttachmentError;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::chat::support;

pub fn map_error(error: AttachmentError) -> ApiError {
    let message = error.to_string();
    match error {
        AttachmentError::NoSuchAttachment
        | AttachmentError::UnknownAttachment
        | AttachmentError::InvalidId => ApiError::not_found("no_such_attachment"),
        AttachmentError::UnknownMessage => ApiError::not_found("no_such_message"),
        AttachmentError::AlreadyLinked => {
            ApiError::coded(StatusCode::CONFLICT, "already_linked", message)
        }
        AttachmentError::NotUploading => {
            ApiError::coded(StatusCode::CONFLICT, "not_uploading", message)
        }
        AttachmentError::NotReady => ApiError::coded(StatusCode::CONFLICT, "not_ready", message),
        AttachmentError::OffsetMismatch { next_offset } => {
            let mut refused = ApiError::coded(StatusCode::CONFLICT, "offset_mismatch", message);
            refused.body["detail"]["next_offset"] = json!(next_offset);
            refused
        }
        AttachmentError::ChunkTooLarge => {
            ApiError::coded(StatusCode::PAYLOAD_TOO_LARGE, "chunk_too_large", message)
        }
        AttachmentError::FileTooLarge => {
            ApiError::coded(StatusCode::PAYLOAD_TOO_LARGE, "file_too_large", message)
        }
        AttachmentError::ConversationQuota => {
            ApiError::coded(StatusCode::PAYLOAD_TOO_LARGE, "conversation_quota", message)
        }
        AttachmentError::HostQuota => {
            ApiError::coded(StatusCode::PAYLOAD_TOO_LARGE, "host_quota", message)
        }
        AttachmentError::InvalidName => ApiError::invalid_field("invalid_name", "name", message),
        AttachmentError::InvalidSha256 => {
            ApiError::invalid_field("invalid_sha256", "sha256", message)
        }
        AttachmentError::TooManyAttachments => {
            ApiError::invalid_field("too_many_attachments", "attachments", message)
        }
        AttachmentError::Chat(error) => support::map_error(error),
        AttachmentError::Database(error) => ApiError::internal(error),
        AttachmentError::Io(error) => ApiError::internal(error),
    }
}
