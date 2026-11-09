//! Attachments of 3 MiB or more, sent through a Graph upload session.
//!
//! The file is read from disk one chunk at a time, so memory holds one chunk.
//! Every chunk is a multiple of 320 KiB except the last, and none is above
//! 4 MiB. The `uploadUrl` is the credential: it goes out without an
//! `Authorization` header and lives in a [`Secret`], so no error or log line
//! can show it. A chunk that fails is retried up to three times, each time
//! from the offset Graph reports in `nextExpectedRanges`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::draft::AttachmentMeta;
use crate::error::MailError;
use crate::graph::{check_status, encode_segment, parse_attachment, GraphClient, GRAPH_BASE};
use crate::paths::{unreadable, CheckedFile};
use crate::redact::{Redacted, Secret};

/// Graph's granularity for upload sessions.
pub const UPLOAD_GRANULE_BYTES: usize = 320 * 1024;
/// Chunk size by default: twelve granules, 3.75 MiB.
pub const DEFAULT_UPLOAD_CHUNK_BYTES: usize = 12 * UPLOAD_GRANULE_BYTES;
/// Chunk ceiling. Nothing larger is sent.
pub const MAX_UPLOAD_CHUNK_BYTES: usize = 4 * 1024 * 1024;

/// Failed chunks retried in a row. The one after them stops the upload.
const MAX_RETRIES: u32 = 3;
const BACKOFF_BASE: Duration = Duration::from_millis(250);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
    upload_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStatus {
    #[serde(default)]
    next_expected_ranges: Vec<String>,
}

#[derive(Deserialize)]
struct RawList {
    #[serde(default)]
    value: Vec<RawListed>,
}

#[derive(Deserialize)]
struct RawListed {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
}

/// What a chunk answer means for the loop.
enum Sent {
    Accepted,
    /// Graph's last answer, which for Outlook attachments is often an empty 201.
    Finished(Option<AttachmentMeta>),
}

enum ChunkFailure {
    /// Network trouble, a 416, a 429 or a 5xx: ask the session where to resume.
    Retry,
    Fatal(MailError),
}

impl GraphClient<'_> {
    /// Uploads `file` as attachment `name` of `draft_id` and returns what Graph created.
    pub(crate) async fn upload_session(
        &self,
        draft_id: &str,
        name: &str,
        file: &CheckedFile,
    ) -> Result<AttachmentMeta, MailError> {
        self.owned(draft_id)?;
        let url = self.open_session(draft_id, name, file.size).await?;
        let mut source = ChunkSource::open(&file.path)?;
        let mut offset = 0;
        let mut failures = 0;
        loop {
            if offset >= file.size {
                return Err(upload_failed(
                    "the session has every byte but no attachment",
                ));
            }
            let len = chunk_len(offset, file.size);
            match self.put_chunk(&url, file, &mut source, offset, len).await {
                Ok(Sent::Accepted) => {
                    offset += len as u64;
                    failures = 0;
                }
                Ok(Sent::Finished(found)) => {
                    let meta = match found {
                        Some(meta) => meta,
                        None => self.find_uploaded(draft_id, name).await?,
                    };
                    self.record_attachment(draft_id, &file.sha256, &meta)?;
                    return Ok(meta);
                }
                Err(ChunkFailure::Fatal(err)) => return Err(err),
                Err(ChunkFailure::Retry) => {
                    failures += 1;
                    if failures > MAX_RETRIES {
                        return Err(upload_failed(format!(
                            "the chunk at byte {offset} failed {failures} times in a row"
                        )));
                    }
                    tokio::time::sleep(BACKOFF_BASE * 2u32.pow(failures - 1)).await;
                    offset = self.next_expected(&url).await?;
                }
            }
        }
    }

    async fn open_session(
        &self,
        draft_id: &str,
        name: &str,
        size: u64,
    ) -> Result<Secret, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments/createUploadSession",
            encode_segment(draft_id)
        );
        let body = json!({
            "AttachmentItem": {"attachmentType": "file", "name": name, "size": size},
        });
        let response = self
            .transport
            .post_json(&url, self.access_token, &body)
            .await?;
        check_status(&response)?;
        let raw: RawSession = serde_json::from_str(&response.body).map_err(|_| {
            MailError::BadResponse(Redacted::new("the upload session answer is incomplete"))
        })?;
        Ok(Secret::new(raw.upload_url))
    }

    async fn put_chunk(
        &self,
        url: &Secret,
        file: &CheckedFile,
        source: &mut ChunkSource,
        offset: u64,
        len: usize,
    ) -> Result<Sent, ChunkFailure> {
        let bytes = source.chunk(offset, len).map_err(ChunkFailure::Fatal)?;
        let last = offset + len as u64 == file.size;
        let content_range = format!("bytes {offset}-{}/{}", offset + len as u64 - 1, file.size);
        let response = self
            .transport
            .put_range(url.expose(), &content_range, bytes)
            .await
            .map_err(|_| ChunkFailure::Retry)?;
        match response.status {
            202 => Ok(Sent::Accepted),
            200 | 201 if last && response.body.trim().is_empty() => Ok(Sent::Finished(None)),
            200 | 201 if last => Ok(Sent::Finished(Some(
                parse_attachment(&response.body).map_err(ChunkFailure::Fatal)?,
            ))),
            416 | 429 | 500..=599 => Err(ChunkFailure::Retry),
            status => Err(ChunkFailure::Fatal(upload_failed(format!(
                "Graph refused a chunk with HTTP {status}"
            )))),
        }
    }

    /// The attachment a bodiless final answer created: the last one of the draft with that name.
    async fn find_uploaded(&self, draft_id: &str, name: &str) -> Result<AttachmentMeta, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments?$select=id,name,size",
            encode_segment(draft_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let list: RawList = serde_json::from_str(&response.body)
            .map_err(|_| upload_failed("the attachment list is unreadable"))?;
        list.value
            .into_iter()
            .rev()
            .find(|item| item.name == name)
            .map(|item| AttachmentMeta {
                id: item.id,
                name: item.name,
                size: item.size,
            })
            .ok_or_else(|| upload_failed("the upload finished but the attachment is not on the draft"))
    }

    /// The byte Graph expects next, from the session status.
    async fn next_expected(&self, url: &Secret) -> Result<u64, MailError> {
        let response = self.transport.get_upload_status(url.expose()).await?;
        check_status(&response)?;
        let status: RawStatus = serde_json::from_str(&response.body)
            .map_err(|_| upload_failed("the upload status is unreadable"))?;
        status
            .next_expected_ranges
            .first()
            .and_then(|range| range.split('-').next())
            .and_then(|start| start.parse().ok())
            .ok_or_else(|| upload_failed("the upload status names no next byte"))
    }
}

/// Reads the file one chunk at a time, so memory never holds more than a chunk.
struct ChunkSource {
    file: File,
}

impl ChunkSource {
    fn open(path: &Path) -> Result<Self, MailError> {
        let file = File::open(path).map_err(unreadable)?;
        Ok(Self { file })
    }

    fn chunk(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, MailError> {
        let mut bytes = vec![0; len];
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(unreadable)?;
        self.file.read_exact(&mut bytes).map_err(unreadable)?;
        Ok(bytes)
    }
}

/// The chunk starting at `offset`: the default size, or what is left of the file.
fn chunk_len(offset: u64, total: u64) -> usize {
    (total - offset).min(DEFAULT_UPLOAD_CHUNK_BYTES as u64) as usize
}

fn upload_failed(reason: impl Into<String>) -> MailError {
    MailError::UploadFailed(Redacted::new(reason))
}
