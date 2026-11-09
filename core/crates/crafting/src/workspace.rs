//! The private folder where a CLI works on a copy of an attachment, and the
//! file-name rule for that copy.

use crate::{ooxml, CraftError, Document, Kind};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Largest attachment the chat hands to a CLI (the same limit as the upload).
pub const MAX_INPUT_BYTES: u64 = 10 * 1024 * 1024;
const MAX_STEM_CHARS: usize = 80;

/// A folder under `base` that is removed, with everything in it, when dropped.
pub struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    pub fn create(base: &Path) -> Result<Self, CraftError> {
        let dir = base.join(format!("craft-{}", ulid::Ulid::generate()));
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Copies the attachment in under a safe name. Office packages are checked
    /// first, and nothing is copied when the check fails.
    pub fn stage(&self, doc: &Document, kind: Kind) -> Result<PathBuf, CraftError> {
        let mut bytes = Vec::new();
        File::open(&doc.path)?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(CraftError::BadDocument("el archivo pasa de 10 MiB"));
        }
        if kind.is_ooxml() {
            ooxml::check(&bytes)?;
        }
        let target = self.join(&safe_name(&doc.name, kind.extension()));
        fs::write(&target, &bytes)?;
        Ok(target)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A file name for the working copy: only the base name survives, the
/// characters Windows refuses become `_`, dots do not survive in the stem,
/// reserved device names get a prefix, and the stem is bounded. The extension
/// is the one the CLI needs, whatever the original said.
pub fn safe_name(original: &str, extension: &str) -> String {
    let base = original.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
    let cleaned: String = stem.chars().map(clean_char).take(MAX_STEM_CHARS).collect();
    let cleaned = cleaned.trim_matches(' ');
    let stem = if cleaned.is_empty() {
        "documento".to_string()
    } else if is_reserved(cleaned) {
        format!("_{cleaned}")
    } else {
        cleaned.to_string()
    };
    format!("{stem}.{extension}")
}

fn clean_char(c: char) -> char {
    if c.is_control()
        || matches!(
            c,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '.'
        )
    {
        '_'
    } else {
        c
    }
}

fn is_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (bytes.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(bytes[3], b'1'..=b'9'))
}
