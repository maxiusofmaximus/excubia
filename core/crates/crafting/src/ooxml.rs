//! Checks an Office package from its ZIP directory alone, before a CLI unpacks
//! it: entry count, declared sizes and entry names. Declared sizes can lie, so
//! the CLIs also run under a time limit.

use crate::CraftError;

pub const MAX_ENTRIES: usize = 5_000;
pub const MAX_DECLARED_BYTES: u64 = 200 * 1024 * 1024;

const END_SIGNATURE: u32 = 0x0605_4b50;
const END_LEN: usize = 22;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const CENTRAL_LEN: usize = 46;
const CONTENT_TYPES: &[u8] = b"[Content_Types].xml";

/// `Ok` when the bytes are a ZIP with a sane directory that holds `[Content_Types].xml`.
pub fn check(bytes: &[u8]) -> Result<(), CraftError> {
    let end = find_end(bytes).ok_or_else(|| bad("no es un ZIP válido"))?;
    let entries = usize::from(u16_at(bytes, end + 10)?);
    if entries > MAX_ENTRIES {
        return Err(bad("demasiadas entradas"));
    }
    let mut pos = u32_at(bytes, end + 16)? as usize;
    let mut declared: u64 = 0;
    let mut has_content_types = false;
    for _ in 0..entries {
        if u32_at(bytes, pos)? != CENTRAL_SIGNATURE {
            return Err(bad("directorio central dañado"));
        }
        declared += u64::from(u32_at(bytes, pos + 24)?);
        if declared > MAX_DECLARED_BYTES {
            return Err(bad("demasiado grande al descomprimir"));
        }
        let name_len = usize::from(u16_at(bytes, pos + 28)?);
        let name = slice(bytes, pos + CENTRAL_LEN, name_len)?;
        if !entry_is_safe(name) {
            return Err(bad("nombre de entrada no permitido"));
        }
        has_content_types |= name == CONTENT_TYPES;
        let extra_len = usize::from(u16_at(bytes, pos + 30)?);
        let comment_len = usize::from(u16_at(bytes, pos + 32)?);
        pos += CENTRAL_LEN + name_len + extra_len + comment_len;
    }
    if has_content_types {
        Ok(())
    } else {
        Err(bad("falta [Content_Types].xml"))
    }
}

/// The end-of-directory record is the last 22 bytes plus a comment whose
/// length must reach exactly the end of the file.
fn find_end(bytes: &[u8]) -> Option<usize> {
    let last = bytes.len().checked_sub(END_LEN)?;
    let first = last.saturating_sub(usize::from(u16::MAX));
    (first..=last).rev().find(|&at| {
        u32_at(bytes, at).ok() == Some(END_SIGNATURE)
            && u16_at(bytes, at + 20).ok().map(usize::from) == Some(bytes.len() - at - END_LEN)
    })
}

/// No parent folders and no absolute paths: an entry name is only ever a relative path.
fn entry_is_safe(name: &[u8]) -> bool {
    !name.is_empty()
        && !matches!(name.first(), Some(b'/' | b'\\'))
        && !name.windows(2).any(|pair| pair == b"..")
}

fn bad(reason: &'static str) -> CraftError {
    CraftError::BadDocument(reason)
}

fn slice(bytes: &[u8], at: usize, len: usize) -> Result<&[u8], CraftError> {
    at.checked_add(len)
        .and_then(|end| bytes.get(at..end))
        .ok_or_else(|| bad("directorio central cortado"))
}

fn array<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], CraftError> {
    slice(bytes, at, N)?
        .try_into()
        .map_err(|_| bad("directorio central cortado"))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, CraftError> {
    Ok(u16::from_le_bytes(array(bytes, at)?))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, CraftError> {
    Ok(u32::from_le_bytes(array(bytes, at)?))
}
