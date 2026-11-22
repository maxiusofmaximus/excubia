//! The arithmetic of one attachment upload: which bytes the next chunk takes,
//! how far the host has got, and whether a refused chunk is worth another try.
//! Dart reads the file at the offsets given here and sends the chunks in order.

use sha2::{Digest, Sha256};

use super::attach_model::AttachmentFailure;

/// Raw bytes per chunk: 128 KiB (`api.md`, Attachments).
pub const CHUNK_BYTES: u64 = 128 * 1024;
/// The largest input the host takes: 10 MiB.
pub const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;
/// Refusals in a row, without an accepted chunk, before the upload gives up:
/// three attempts at one chunk.
const MAX_REFUSALS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadCursor {
    pub size: u64,
    /// The bytes the host holds, which is the offset of the next chunk.
    pub sent: u64,
    pub refusals: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkSpan {
    pub offset: u64,
    pub len: u64,
}

#[flutter_rust_bridge::frb(sync)]
pub fn upload_start(size: u64) -> UploadCursor {
    UploadCursor {
        size,
        sent: 0,
        refusals: 0,
    }
}

/// The next chunk, or `None` once every byte is with the host.
#[flutter_rust_bridge::frb(sync)]
pub fn upload_next(cursor: UploadCursor) -> Option<ChunkSpan> {
    let left = cursor.size.saturating_sub(cursor.sent);
    (left > 0).then_some(ChunkSpan {
        offset: cursor.sent,
        len: left.min(CHUNK_BYTES),
    })
}

/// Share of the file the host holds, from 0 to 1. An empty file is already whole.
#[flutter_rust_bridge::frb(sync)]
pub fn upload_progress(cursor: UploadCursor) -> f64 {
    if cursor.size == 0 {
        return 1.0;
    }
    cursor.sent.min(cursor.size) as f64 / cursor.size as f64
}

/// The host took the chunk and answered with its new `next_offset`.
#[flutter_rust_bridge::frb(sync)]
pub fn upload_accepted(cursor: UploadCursor, next_offset: u64) -> UploadCursor {
    UploadCursor {
        sent: next_offset,
        refusals: 0,
        ..cursor
    }
}

/// The cursor to resume from after a refused chunk, or `None` when the upload
/// ends. An offset mismatch resumes from the host's `next_offset`; a transport
/// fault, a `429` or a `5xx` retries the same chunk. Anything else ends it.
#[flutter_rust_bridge::frb(sync)]
pub fn upload_refused(cursor: UploadCursor, failure: AttachmentFailure) -> Option<UploadCursor> {
    let refusals = cursor.refusals.saturating_add(1);
    let (sent, retriable) = if failure.code == "offset_mismatch" {
        (failure.next_offset.unwrap_or(cursor.sent), true)
    } else {
        (cursor.sent, is_transient(failure.status))
    };
    if !retriable || refusals >= MAX_REFUSALS || sent > cursor.size {
        return None;
    }
    Some(UploadCursor {
        sent,
        refusals,
        ..cursor
    })
}

fn is_transient(status: u16) -> bool {
    status == 0 || status == 429 || status >= 500
}

/// SHA-256 of a whole file, read in pieces so a 10 MiB image is never held
/// twice. `None` when the file cannot be read.
pub fn attachment_sha256(path: String) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(code: &str, status: u16, next_offset: Option<u64>) -> AttachmentFailure {
        AttachmentFailure {
            code: code.to_string(),
            status,
            field: None,
            next_offset,
            retry_after: None,
        }
    }

    /// Sends every chunk the cursor offers and returns their lengths.
    fn chunk_lengths(size: u64) -> Vec<u64> {
        let mut cursor = upload_start(size);
        let mut lengths = Vec::new();
        while let Some(span) = upload_next(cursor) {
            assert_eq!(span.offset, cursor.sent);
            lengths.push(span.len);
            cursor = upload_accepted(cursor, span.offset + span.len);
        }
        lengths
    }

    #[test]
    fn an_empty_file_has_no_chunk_and_is_already_whole() {
        assert_eq!(upload_next(upload_start(0)), None);
        assert_eq!(upload_progress(upload_start(0)), 1.0);
    }

    #[test]
    fn chunks_are_at_most_128_kib_and_cover_the_file() {
        assert_eq!(chunk_lengths(CHUNK_BYTES), vec![CHUNK_BYTES]);
        assert_eq!(chunk_lengths(CHUNK_BYTES + 1), vec![CHUNK_BYTES, 1]);
        let ten_mib = chunk_lengths(MAX_ATTACHMENT_BYTES);
        assert_eq!(ten_mib.len(), 80);
        assert!(ten_mib.iter().all(|len| *len <= CHUNK_BYTES));
        assert_eq!(ten_mib.iter().sum::<u64>(), MAX_ATTACHMENT_BYTES);
    }

    #[test]
    fn progress_follows_the_host_offset() {
        let cursor = upload_accepted(upload_start(4 * CHUNK_BYTES), CHUNK_BYTES);
        assert_eq!(upload_progress(cursor), 0.25);
        assert_eq!(
            upload_next(cursor).map(|span| span.offset),
            Some(CHUNK_BYTES)
        );
    }

    #[test]
    fn an_offset_mismatch_resumes_from_the_host_offset() {
        let cursor = upload_start(MAX_ATTACHMENT_BYTES);
        let failure = refusal("offset_mismatch", 409, Some(CHUNK_BYTES));
        let resumed = upload_refused(cursor, failure).expect("a mismatch resumes");
        assert_eq!(
            upload_next(resumed).map(|span| span.offset),
            Some(CHUNK_BYTES)
        );
        assert_eq!(resumed.refusals, 1);
    }

    #[test]
    fn a_transient_refusal_retries_the_same_chunk_three_times_at_most() {
        let cursor = upload_accepted(upload_start(MAX_ATTACHMENT_BYTES), CHUNK_BYTES);
        let mut retry = upload_refused(cursor, refusal("offline", 0, None));
        let mut attempts = 1;
        while let Some(again) = retry {
            assert_eq!(again.sent, CHUNK_BYTES);
            attempts += 1;
            retry = upload_refused(again, refusal("http_503", 503, None));
        }
        assert_eq!(attempts, 3);
        assert_eq!(retry, None);
    }

    #[test]
    fn a_contract_refusal_gives_up_at_once_and_an_accepted_chunk_resets_the_count() {
        let cursor = upload_start(MAX_ATTACHMENT_BYTES);
        assert_eq!(
            upload_refused(cursor, refusal("invalid_base64", 422, None)),
            None
        );
        let once = upload_refused(cursor, refusal("http_500", 500, None)).expect("retry");
        assert_eq!(once.refusals, 1);
        assert_eq!(upload_accepted(once, 0).refusals, 0);
    }

    #[test]
    fn a_next_offset_past_the_file_gives_up() {
        let cursor = upload_start(10);
        let failure = refusal("offset_mismatch", 409, Some(11));
        assert_eq!(upload_refused(cursor, failure), None);
    }

    #[test]
    fn the_sha256_of_a_file_is_streamed_in_hex() {
        let path = std::env::temp_dir().join(format!("att7a-sha-{}.bin", std::process::id()));
        std::fs::write(&path, b"abc").expect("write");
        let digest = attachment_sha256(path.to_string_lossy().into_owned());
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(
            digest.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(attachment_sha256(path.to_string_lossy().into_owned()), None);
    }
}
