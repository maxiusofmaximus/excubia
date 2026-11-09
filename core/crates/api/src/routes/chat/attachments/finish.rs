//! `complete`: the host checks the bytes it holds against what was declared, then
//! stores a listed document as sent, or re-encodes an image in an isolated worker.

use std::time::Duration;

use hn_core_chat::{Attachment, AttachmentFiles, AttachmentState, ReadyFile};
use hn_core_media::{process_image_isolated, Limits, MediaError, ProcessedImage};
use sha2::{Digest, Sha256};

use crate::error::ApiError;
use crate::ApiContext;

use super::documents;
use super::errors::map_error;
use super::{file_store, owned_attachment};

/// A slow image is refused rather than held: the worker gets this long.
const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn finish_upload(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
) -> Result<Attachment, ApiError> {
    let attachment = owned_attachment(ctx, conversation_id, attachment_id)?;
    if attachment.state == AttachmentState::Ready {
        return Ok(attachment);
    }
    match process_and_store(ctx, conversation_id, attachment_id, &attachment).await {
        Ok(()) => ctx.chat.attachment(attachment_id).map_err(map_error),
        Err(error) => {
            // A concurrent `complete` may have finished this upload first.
            let ready = ctx
                .chat
                .attachment(attachment_id)
                .ok()
                .filter(|current| current.state == AttachmentState::Ready);
            match ready {
                Some(current) => Ok(current),
                None => {
                    discard(ctx, conversation_id, attachment_id);
                    Err(error)
                }
            }
        }
    }
}

async fn process_and_store(
    ctx: &ApiContext,
    conversation_id: &str,
    attachment_id: &str,
    attachment: &Attachment,
) -> Result<(), ApiError> {
    let files = file_store(ctx)?;
    let bytes = files
        .read_part(conversation_id, attachment_id)
        .map_err(map_error)?;
    if bytes.len() as u64 != attachment.declared_size {
        return Err(ApiError::invalid_field(
            "size_mismatch",
            "size",
            "the upload is not the declared size",
        ));
    }
    if hex::encode(Sha256::digest(&bytes)) != attachment.declared_sha256 {
        return Err(ApiError::invalid_field(
            "hash_mismatch",
            "sha256",
            "the upload does not match the declared sha256",
        ));
    }
    match documents::document_mime(&attachment.name, &bytes)? {
        Some(mime) => {
            let ready = ReadyFile {
                mime: mime.to_owned(),
                width: 0,
                height: 0,
                size: bytes.len() as u64,
                sha256: attachment.declared_sha256.clone(),
            };
            store_ready(ctx, &files, conversation_id, attachment_id, &bytes, &ready)
        }
        None => {
            let processed = process_image(bytes).await?;
            let ready = ReadyFile {
                mime: processed.mime.to_owned(),
                width: processed.width,
                height: processed.height,
                size: processed.bytes.len() as u64,
                sha256: processed.sha256,
            };
            store_ready(
                ctx,
                &files,
                conversation_id,
                attachment_id,
                &processed.bytes,
                &ready,
            )
        }
    }
}

async fn process_image(bytes: Vec<u8>) -> Result<ProcessedImage, ApiError> {
    tokio::task::spawn_blocking(move || {
        process_image_isolated(&bytes, Limits::default(), PROCESS_TIMEOUT)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(media_error)
}

fn store_ready(
    ctx: &ApiContext,
    files: &AttachmentFiles,
    conversation_id: &str,
    attachment_id: &str,
    bytes: &[u8],
    ready: &ReadyFile,
) -> Result<(), ApiError> {
    files
        .finish(conversation_id, attachment_id, bytes)
        .map_err(map_error)?;
    ctx.chat.mark_ready(attachment_id, ready).map_err(map_error)
}

fn media_error(error: MediaError) -> ApiError {
    match error {
        MediaError::WorkerFailed => ApiError::internal(error),
        other => ApiError::invalid_field(other.code(), "file", other.to_string()),
    }
}

/// Drop a failed upload: its row and its bytes. The answer is already decided,
/// so a failure here is only logged.
fn discard(ctx: &ApiContext, conversation_id: &str, attachment_id: &str) {
    if let Err(error) = ctx.chat.delete_unlinked(conversation_id, &[attachment_id]) {
        eprintln!("attachment {attachment_id}: row not dropped: {error}");
    }
    let Ok(files) = file_store(ctx) else {
        return;
    };
    if let Err(error) = files.remove(conversation_id, attachment_id) {
        eprintln!("attachment {attachment_id}: bytes not removed: {error}");
    }
}
