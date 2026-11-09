//! The files of the attachments: one folder per conversation, one file per id.
//! The user's name never reaches the disk, and the ids are checked before they
//! become path components, so none of them can leave the folder.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use hn_core_persist::write_atomic;

use crate::error::AttachmentError;
use crate::model_attachment::{CHUNK_MAX_BYTES, FILE_MAX_BYTES};

const CONVERSATION_PREFIX: &str = "cnv_";
const ATTACHMENT_PREFIX: &str = "att_";
const PART_SUFFIX: &str = ".part";
/// The folder, inside a conversation's, that holds the page previews of its files.
const PREVIEWS_DIR: &str = "previews";

/// Clones share the write lock: the offset check and the append must not
/// interleave with another chunk of the same upload.
#[derive(Clone, Debug)]
pub struct AttachmentFiles {
    root: PathBuf,
    writes: Arc<Mutex<()>>,
}

impl AttachmentFiles {
    /// `root` is the folder that holds one subfolder per conversation.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            writes: Arc::new(Mutex::new(())),
        }
    }

    /// Append `data` to the upload in progress. It is accepted only when
    /// `offset` is the number of bytes already stored; the result is the new
    /// offset.
    pub fn append_chunk(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<u64, AttachmentError> {
        let _writing = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        let part = self.part_path(conversation_id, attachment_id)?;
        let stored = match fs::metadata(&part) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if offset != stored {
            return Err(AttachmentError::OffsetMismatch {
                next_offset: stored,
            });
        }
        if data.len() > CHUNK_MAX_BYTES {
            return Err(AttachmentError::ChunkTooLarge);
        }
        let total = stored + data.len() as u64;
        if total > FILE_MAX_BYTES {
            return Err(AttachmentError::FileTooLarge);
        }
        self.ensure_conversation_dir(conversation_id)?;
        let mut file = OpenOptions::new().create(true).append(true).open(&part)?;
        file.write_all(data)?;
        Ok(total)
    }

    /// Store the final content, the one the host processed, and drop the
    /// upload in progress.
    pub fn finish(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        final_bytes: &[u8],
    ) -> Result<(), AttachmentError> {
        if final_bytes.len() as u64 > FILE_MAX_BYTES {
            return Err(AttachmentError::FileTooLarge);
        }
        self.ensure_conversation_dir(conversation_id)?;
        let path = self.file_path(conversation_id, attachment_id)?;
        write_atomic(&path, final_bytes)?;
        remove_if_exists(&self.part_path(conversation_id, attachment_id)?)
    }

    /// Remove the stored file, its previews and any upload in progress of one attachment.
    pub fn remove(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<(), AttachmentError> {
        remove_if_exists(&self.part_path(conversation_id, attachment_id)?)?;
        remove_if_exists(&self.file_path(conversation_id, attachment_id)?)?;
        self.remove_previews(conversation_id, attachment_id)
    }

    /// The PNG of one page of a stored file, if it was rendered before.
    pub fn read_preview(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        page: u32,
    ) -> Result<Option<Vec<u8>>, AttachmentError> {
        match fs::read(self.preview_path(conversation_id, attachment_id, page)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Keep the PNG of one page of a stored file for the next request.
    pub fn write_preview(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        page: u32,
        png: &[u8],
    ) -> Result<(), AttachmentError> {
        let path = self.preview_path(conversation_id, attachment_id, page)?;
        fs::create_dir_all(self.previews_dir(conversation_id)?)?;
        write_atomic(&path, png)?;
        Ok(())
    }

    /// The page count of a stored file, when the renderer told it before.
    pub fn read_pages(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<Option<u32>, AttachmentError> {
        match fs::read_to_string(self.pages_path(conversation_id, attachment_id)?) {
            Ok(text) => Ok(text.trim().parse().ok()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Keep the page count of a stored file beside its previews, for the cached answers.
    pub fn write_pages(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        pages: u32,
    ) -> Result<(), AttachmentError> {
        let path = self.pages_path(conversation_id, attachment_id)?;
        fs::create_dir_all(self.previews_dir(conversation_id)?)?;
        write_atomic(&path, pages.to_string().as_bytes())?;
        Ok(())
    }

    /// The bytes sent so far of an upload. Empty when nothing was stored yet.
    pub fn read_part(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, AttachmentError> {
        let part = self.part_path(conversation_id, attachment_id)?;
        match fs::read(part) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// The final bytes of a ready attachment.
    pub fn read_file(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, AttachmentError> {
        Ok(fs::read(self.file_path(conversation_id, attachment_id)?)?)
    }

    /// Remove the folder of a conversation with everything in it.
    pub fn remove_conversation(&self, conversation_id: &str) -> Result<(), AttachmentError> {
        let dir = self.conversation_dir(conversation_id)?;
        match fs::remove_dir_all(dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn conversation_dir(&self, conversation_id: &str) -> Result<PathBuf, AttachmentError> {
        Ok(self
            .root
            .join(checked(conversation_id, CONVERSATION_PREFIX)?))
    }

    fn previews_dir(&self, conversation_id: &str) -> Result<PathBuf, AttachmentError> {
        Ok(self.conversation_dir(conversation_id)?.join(PREVIEWS_DIR))
    }

    /// Previews are named by attachment and page, so the attachment's own id is their prefix.
    fn preview_path(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        page: u32,
    ) -> Result<PathBuf, AttachmentError> {
        let attachment = checked(attachment_id, ATTACHMENT_PREFIX)?;
        Ok(self
            .previews_dir(conversation_id)?
            .join(format!("{attachment}-p{page}.png")))
    }

    /// The page count sits beside the previews, named so that its name never ends in `.png`.
    fn pages_path(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<PathBuf, AttachmentError> {
        let attachment = checked(attachment_id, ATTACHMENT_PREFIX)?;
        Ok(self
            .previews_dir(conversation_id)?
            .join(format!("{attachment}-pages")))
    }

    fn remove_previews(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<(), AttachmentError> {
        let prefix = format!("{}-p", checked(attachment_id, ATTACHMENT_PREFIX)?);
        let entries = match fs::read_dir(self.previews_dir(conversation_id)?) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                remove_if_exists(&entry.path())?;
            }
        }
        Ok(())
    }

    fn ensure_conversation_dir(&self, conversation_id: &str) -> Result<(), AttachmentError> {
        fs::create_dir_all(self.conversation_dir(conversation_id)?)?;
        Ok(())
    }

    /// Where a stored file lives: the host's path, which the agent is told.
    pub fn file_path(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<PathBuf, AttachmentError> {
        let attachment = checked(attachment_id, ATTACHMENT_PREFIX)?;
        Ok(self.conversation_dir(conversation_id)?.join(attachment))
    }

    fn part_path(
        &self,
        conversation_id: &str,
        attachment_id: &str,
    ) -> Result<PathBuf, AttachmentError> {
        let mut name = self
            .file_path(conversation_id, attachment_id)?
            .into_os_string();
        name.push(PART_SUFFIX);
        Ok(PathBuf::from(name))
    }
}

/// An id is its prefix and 26 uppercase ULID characters: nothing that could be a
/// separator, a drive or a parent folder.
fn checked<'a>(id: &'a str, prefix: &str) -> Result<&'a str, AttachmentError> {
    let body = id.strip_prefix(prefix).ok_or(AttachmentError::InvalidId)?;
    let well_formed = body.len() == 26
        && body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
    if well_formed {
        Ok(id)
    } else {
        Err(AttachmentError::InvalidId)
    }
}

fn remove_if_exists(path: &Path) -> Result<(), AttachmentError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_ids_are_refused_before_any_path_is_built() {
        for hostile in ["../x", "a/b", "C:\\x", "att_../../x", ""] {
            assert!(checked(hostile, ATTACHMENT_PREFIX).is_err(), "{hostile}");
        }
        assert!(checked("att_01J5Z8K2M3N4P5Q6R7S8T9V0WX", ATTACHMENT_PREFIX).is_ok());
    }
}
