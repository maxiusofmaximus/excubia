//! The attachments of a chat: the row the store keeps, the limits the decided
//! plan sets (`PLAN-CHAT-ATTACH.md`), and the checks on what the user sends.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::Row;
use serde::Serialize;

use crate::error::AttachmentError;

/// Files a single message can carry.
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 4;
/// Bytes of one uploaded chunk, before base64.
pub const CHUNK_MAX_BYTES: usize = 128 * 1024;
/// Bytes of one stored file.
pub const FILE_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Bytes that one conversation may keep in attachments.
pub const CONVERSATION_QUOTA_BYTES: u64 = 100 * 1024 * 1024;
/// Bytes that the whole host may keep in attachments.
pub const HOST_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ATTACHMENT_NAME_CHARS: usize = 120;
/// The stored text of a message that carries files and no words.
pub const ATTACHMENT_ONLY_TEXT: &str = "(adjunto)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentState {
    Uploading,
    Ready,
}

impl FromSql for AttachmentState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "uploading" => Ok(Self::Uploading),
            "ready" => Ok(Self::Ready),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

/// One attachment as the store keeps it. `size`, `sha256`, `mime` and the
/// dimensions are set once the file is `ready`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub conversation_id: String,
    pub client_id: String,
    pub name: String,
    pub declared_size: u64,
    pub declared_sha256: String,
    pub state: AttachmentState,
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size: Option<u64>,
    pub sha256: Option<String>,
    pub message_id: Option<String>,
    pub created_at: String,
}

/// What a ready file is: an image the host re-encoded, or a document kept as sent.
/// A document has no dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Image,
    Document,
}

impl FileKind {
    pub fn of(width: u32, height: u32) -> Self {
        if width == 0 && height == 0 {
            Self::Document
        } else {
            Self::Image
        }
    }
}

/// Whether the host can draw a page preview of a file: a document named as Excel, Word or PDF.
pub fn previewable(name: &str, kind: FileKind) -> bool {
    kind == FileKind::Document
        && name.rsplit_once('.').is_some_and(|(_, extension)| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "xlsx" | "docx" | "pdf"
            )
        })
}

/// A ready file as a message shows it, in the timeline and in the agent's context.
/// The user's name is shown to the user only; the prompt never carries it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkedFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    #[serde(rename = "w")]
    pub width: u32,
    #[serde(rename = "h")]
    pub height: u32,
    pub kind: FileKind,
    pub previewable: bool,
}

/// What `mark_ready` stores: the content as the host decoded and re-encoded it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyFile {
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub sha256: String,
}

/// The column list every read of `attachment` uses; `Attachment::from_row`
/// reads it in this order.
pub(crate) const COLUMNS: &str = "id, conversation_id, client_id, name, declared_size, \
     declared_sha256, state, mime, width, height, final_size, final_sha256, message_id, created_at";

impl Attachment {
    /// Whether the host can draw a page preview of this attachment.
    pub fn previewable(&self) -> bool {
        self.width
            .zip(self.height)
            .is_some_and(|(width, height)| previewable(&self.name, FileKind::of(width, height)))
    }

    pub(crate) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            client_id: row.get(2)?,
            name: row.get(3)?,
            declared_size: unsigned(row, 4)?,
            declared_sha256: row.get(5)?,
            state: row.get(6)?,
            mime: row.get(7)?,
            width: optional_unsigned(row, 8)?,
            height: optional_unsigned(row, 9)?,
            size: optional_unsigned(row, 10)?,
            sha256: row.get(11)?,
            message_id: row.get(12)?,
            created_at: row.get(13)?,
        })
    }
}

/// The message id and the file, from a row of the `linked_files` query: the
/// message id first, then the `COLUMNS` the file needs.
pub(crate) fn linked_from_row(row: &Row<'_>) -> rusqlite::Result<(String, LinkedFile)> {
    let width: u32 = unsigned(row, 5)?;
    let height: u32 = unsigned(row, 6)?;
    let name: String = row.get(2)?;
    let file = LinkedFile {
        id: row.get(1)?,
        previewable: previewable(&name, FileKind::of(width, height)),
        name,
        mime: row.get(3)?,
        size: unsigned(row, 4)?,
        width,
        height,
        kind: FileKind::of(width, height),
    };
    Ok((row.get(0)?, file))
}

fn unsigned<T: TryFrom<i64>>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let raw: i64 = row.get(index)?;
    T::try_from(raw).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, raw))
}

fn optional_unsigned<T: TryFrom<i64>>(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<T>> {
    let raw: Option<i64> = row.get(index)?;
    raw.map(|raw| {
        T::try_from(raw).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, raw))
    })
    .transpose()
}

/// A size as the database stores it. Only sizes within `FILE_MAX_BYTES` reach
/// the table, so the conversion cannot overflow.
pub(crate) fn stored_size(size: u64) -> Result<i64, AttachmentError> {
    if size > FILE_MAX_BYTES {
        return Err(AttachmentError::FileTooLarge);
    }
    i64::try_from(size).map_err(|_| AttachmentError::FileTooLarge)
}

pub(crate) fn check_name(name: &str) -> Result<(), AttachmentError> {
    let fits = !name.is_empty() && name.chars().count() <= MAX_ATTACHMENT_NAME_CHARS;
    if fits && !name.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(AttachmentError::InvalidName)
    }
}

/// The digest is kept as 64 lowercase hex characters.
pub(crate) fn check_sha256(digest: &str) -> Result<String, AttachmentError> {
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(digest.to_ascii_lowercase())
    } else {
        Err(AttachmentError::InvalidSha256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_one_line_and_bounded() {
        assert!(check_name("captura.png").is_ok());
        assert!(check_name("").is_err());
        assert!(check_name("a\nb").is_err());
        assert!(check_name(&"n".repeat(MAX_ATTACHMENT_NAME_CHARS + 1)).is_err());
        assert!(check_name(&"n".repeat(MAX_ATTACHMENT_NAME_CHARS)).is_ok());
    }

    #[test]
    fn digests_are_64_hex_characters() {
        let upper = "AB".repeat(32);
        assert_eq!(check_sha256(&upper).expect("hex"), "ab".repeat(32));
        assert!(check_sha256("zz").is_err());
        assert!(check_sha256(&"g".repeat(64)).is_err());
    }

    #[test]
    fn a_file_without_dimensions_is_a_document() {
        assert_eq!(FileKind::of(0, 0), FileKind::Document);
        assert_eq!(FileKind::of(1, 0), FileKind::Image);
        assert_eq!(FileKind::of(1024, 768), FileKind::Image);
        assert_eq!(
            serde_json::to_value(FileKind::Document).expect("json"),
            "document"
        );
    }

    #[test]
    fn sizes_above_the_file_cap_are_refused() {
        assert_eq!(stored_size(FILE_MAX_BYTES).expect("cap"), 10 * 1024 * 1024);
        assert!(stored_size(FILE_MAX_BYTES + 1).is_err());
    }
}
