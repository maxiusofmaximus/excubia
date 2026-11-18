//! Reads an Office package (a ZIP) from its directory alone, before a CLI unpacks it,
//! and rewrites one part of it. Declared sizes can lie, so the CLIs also run under a
//! time limit; an in-process read is capped by `MAX_PART_BYTES`.

use std::io::{Read, Write};
use std::ops::Range;

use flate2::{Compression, Crc};

use crate::CraftError;

pub const MAX_ENTRIES: usize = 5_000;
pub const MAX_DECLARED_BYTES: u64 = 200 * 1024 * 1024;
/// The largest part read into memory. A bigger one is refused, never cut.
pub const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const END_SIGNATURE: u32 = 0x0605_4b50;
const END_LEN: usize = 22;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;
const STORED: u16 = 0;
const DEFLATE: u16 = 8;
const DATA_DESCRIPTOR: u16 = 0x0008;
const CONTENT_TYPES: &[u8] = b"[Content_Types].xml";

/// One record of the central directory, in the order the package lists it.
pub struct Entry<'a> {
    pub name: &'a [u8],
    flags: u16,
    method: u16,
    crc: u32,
    compressed: u32,
    local: u32,
    record: Range<usize>,
}

/// `Ok` when the bytes are a ZIP with a sane directory that holds `[Content_Types].xml`.
pub fn check(bytes: &[u8]) -> Result<(), CraftError> {
    let entries = directory(bytes)?;
    if entries.iter().any(|entry| !entry_is_safe(entry.name)) {
        return Err(bad("nombre de entrada no permitido"));
    }
    if entries.iter().any(|entry| entry.name == CONTENT_TYPES) {
        Ok(())
    } else {
        Err(bad("falta [Content_Types].xml"))
    }
}

/// Every entry of the central directory. Too many entries, a broken record or a
/// declared size over the cap are refused.
pub fn directory(bytes: &[u8]) -> Result<Vec<Entry<'_>>, CraftError> {
    let end = find_end(bytes).ok_or_else(|| bad("no es un ZIP válido"))?;
    let count = usize::from(u16_at(bytes, end + 10)?);
    if count > MAX_ENTRIES {
        return Err(bad("demasiadas entradas"));
    }
    let mut pos = u32_at(bytes, end + 16)? as usize;
    let mut declared: u64 = 0;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(bytes, pos)? != CENTRAL_SIGNATURE {
            return Err(bad("directorio central dañado"));
        }
        let size = u32_at(bytes, pos + 24)?;
        declared += u64::from(size);
        if declared > MAX_DECLARED_BYTES {
            return Err(bad("demasiado grande al descomprimir"));
        }
        let name_len = usize::from(u16_at(bytes, pos + 28)?);
        let extra_len = usize::from(u16_at(bytes, pos + 30)?);
        let comment_len = usize::from(u16_at(bytes, pos + 32)?);
        let len = CENTRAL_LEN + name_len + extra_len + comment_len;
        entries.push(Entry {
            name: slice(bytes, pos + CENTRAL_LEN, name_len)?,
            flags: u16_at(bytes, pos + 8)?,
            method: u16_at(bytes, pos + 10)?,
            crc: u32_at(bytes, pos + 16)?,
            compressed: u32_at(bytes, pos + 20)?,
            local: u32_at(bytes, pos + 42)?,
            record: slice_range(bytes, pos, len)?,
        });
        pos += len;
    }
    Ok(entries)
}

/// The packed bytes of one entry as the package holds them.
pub fn packed<'a>(bytes: &'a [u8], entry: &Entry<'_>) -> Result<&'a [u8], CraftError> {
    let range = packed_range(bytes, entry)?;
    slice(bytes, range.start, range.len())
}

/// The inflated bytes of one entry, checked against its CRC. A part over
/// `MAX_PART_BYTES` is refused.
pub fn read_part(bytes: &[u8], entry: &Entry<'_>) -> Result<Vec<u8>, CraftError> {
    let data = packed(bytes, entry)?;
    let out = match entry.method {
        STORED => data.to_vec(),
        DEFLATE => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .take(MAX_PART_BYTES + 1)
                .read_to_end(&mut out)
                .map_err(|_| bad("entrada comprimida dañada"))?;
            out
        }
        _ => return Err(bad("compresión no soportada")),
    };
    if out.len() as u64 > MAX_PART_BYTES {
        return Err(bad("una parte del documento es demasiado grande"));
    }
    if crc_of(&out) != entry.crc {
        return Err(bad("suma de control no coincide"));
    }
    Ok(out)
}

/// The package with the entry `name` replaced by `data`, deflated. Every other entry is
/// copied byte for byte, its local record included; only the offsets of the directory move.
pub fn replace_part(bytes: &[u8], name: &[u8], data: &[u8]) -> Result<Vec<u8>, CraftError> {
    let entries = directory(bytes)?;
    let end = find_end(bytes).ok_or_else(|| bad("no es un ZIP válido"))?;
    let target = entries
        .iter()
        .position(|entry| entry.name == name)
        .ok_or_else(|| bad("falta la parte a editar"))?;
    let packed_data = deflate(data)?;
    let crc = crc_of(data);
    let mut out = Vec::with_capacity(bytes.len() + data.len());
    let mut offsets = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        offsets.push(offset_u32(out.len())?);
        if index == target {
            write_local(&mut out, name, &packed_data, crc, data.len())?;
        } else {
            out.extend_from_slice(local_record(bytes, entry)?);
        }
    }
    let directory_at = out.len();
    for (index, entry) in entries.iter().enumerate() {
        let mut record = slice(bytes, entry.record.start, entry.record.len())?.to_vec();
        put_u32(&mut record, 42, offsets[index]);
        if index == target {
            put_u16(&mut record, 8, 0);
            put_u16(&mut record, 10, DEFLATE);
            put_u32(&mut record, 16, crc);
            put_u32(&mut record, 20, offset_u32(packed_data.len())?);
            put_u32(&mut record, 24, offset_u32(data.len())?);
        }
        out.extend_from_slice(&record);
    }
    let directory_len = out.len() - directory_at;
    let mut tail = slice(bytes, end, bytes.len() - end)?.to_vec();
    put_u32(&mut tail, 12, offset_u32(directory_len)?);
    put_u32(&mut tail, 16, offset_u32(directory_at)?);
    out.extend_from_slice(&tail);
    Ok(out)
}

/// Where an entry's packed data sits in the package.
fn packed_range(bytes: &[u8], entry: &Entry<'_>) -> Result<Range<usize>, CraftError> {
    let at = entry.local as usize;
    if u32_at(bytes, at)? != LOCAL_SIGNATURE {
        return Err(bad("registro local dañado"));
    }
    let name_len = usize::from(u16_at(bytes, at + 26)?);
    let extra_len = usize::from(u16_at(bytes, at + 28)?);
    let start = at + LOCAL_LEN + name_len + extra_len;
    slice_range(bytes, start, entry.compressed as usize)
}

/// The whole local record of an entry: its header, name, packed data and, when the
/// entry has one, its data descriptor.
fn local_record<'a>(bytes: &'a [u8], entry: &Entry<'_>) -> Result<&'a [u8], CraftError> {
    let at = entry.local as usize;
    let mut end = packed_range(bytes, entry)?.end;
    if entry.flags & DATA_DESCRIPTOR != 0 {
        end += if u32_at(bytes, end)? == DESCRIPTOR_SIGNATURE {
            16
        } else {
            12
        };
    }
    slice(bytes, at, end.saturating_sub(at))
}

fn write_local(
    out: &mut Vec<u8>,
    name: &[u8],
    packed_data: &[u8],
    crc: u32,
    size: usize,
) -> Result<(), CraftError> {
    let name_len = u16::try_from(name.len()).map_err(|_| bad("nombre demasiado largo"))?;
    out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&DEFLATE.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&offset_u32(packed_data.len())?.to_le_bytes());
    out.extend_from_slice(&offset_u32(size)?.to_le_bytes());
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(packed_data);
    Ok(())
}

fn deflate(data: &[u8]) -> Result<Vec<u8>, CraftError> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

fn crc_of(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

fn offset_u32(value: usize) -> Result<u32, CraftError> {
    u32::try_from(value).map_err(|_| bad("demasiado grande para un ZIP"))
}

fn put_u16(buffer: &mut [u8], at: usize, value: u16) {
    buffer[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buffer: &mut [u8], at: usize, value: u32) {
    buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
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
    let range = slice_range(bytes, at, len)?;
    bytes
        .get(range)
        .ok_or_else(|| bad("directorio central cortado"))
}

fn slice_range(bytes: &[u8], at: usize, len: usize) -> Result<Range<usize>, CraftError> {
    let end = at
        .checked_add(len)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| bad("directorio central cortado"))?;
    Ok(at..end)
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

/// Packages built by hand for the tests of this crate.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A ZIP of `(name, data, deflated)` entries, with the central directory a reader expects.
    pub(crate) fn zip(parts: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflated) in parts {
            let (method, packed) = if *deflated {
                (DEFLATE, deflate(data).expect("deflate"))
            } else {
                (STORED, data.to_vec())
            };
            let at = out.len();
            out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&crc_of(data).to_le_bytes());
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0; 2]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);

            central.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc_of(data).to_le_bytes());
            central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&(at as u32).to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let directory_at = out.len() as u32;
        let directory_len = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&END_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
        out.extend_from_slice(&(parts.len() as u16).to_le_bytes());
        out.extend_from_slice(&directory_len.to_le_bytes());
        out.extend_from_slice(&directory_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::zip;
    use super::*;

    #[test]
    fn a_package_without_content_types_is_refused() {
        let bytes = zip(&[("word/document.xml", b"<w:document/>", false)]);
        assert!(check(&bytes).is_err());
        assert!(check(b"no es un zip").is_err());
    }

    #[test]
    fn a_part_reads_back_stored_and_deflated() {
        let bytes = zip(&[
            ("[Content_Types].xml", b"<Types/>", false),
            ("word/document.xml", b"<w:document>hola</w:document>", true),
        ]);
        check(&bytes).expect("a package");
        let entries = directory(&bytes).expect("directory");
        let document = entries
            .iter()
            .find(|entry| entry.name == b"word/document.xml")
            .expect("document");
        assert_eq!(
            read_part(&bytes, document).expect("read"),
            b"<w:document>hola</w:document>"
        );
    }

    #[test]
    fn a_replaced_part_reads_back_and_the_others_keep_their_bytes() {
        let bytes = zip(&[
            ("[Content_Types].xml", b"<Types/>", true),
            ("word/styles.xml", b"<w:styles>estilo</w:styles>", false),
            ("word/document.xml", b"<w:document>uno</w:document>", true),
        ]);
        let before = directory(&bytes).expect("before");
        let styles_before = before
            .iter()
            .find(|entry| entry.name == b"word/styles.xml")
            .expect("styles");
        let raw_before = local_record(&bytes, styles_before).expect("record").to_vec();

        let out = replace_part(&bytes, b"word/document.xml", b"<w:document>dos</w:document>")
            .expect("replace");
        check(&out).expect("still a package");
        let after = directory(&out).expect("after");
        assert_eq!(after.len(), 3);
        let document = after
            .iter()
            .find(|entry| entry.name == b"word/document.xml")
            .expect("document");
        assert_eq!(
            read_part(&out, document).expect("read"),
            b"<w:document>dos</w:document>"
        );
        let styles = after
            .iter()
            .find(|entry| entry.name == b"word/styles.xml")
            .expect("styles");
        assert_eq!(
            read_part(&out, styles).expect("styles text"),
            b"<w:styles>estilo</w:styles>"
        );
        let raw_after = local_record(&out, styles).expect("record");
        assert_eq!(raw_after, raw_before.as_slice(), "untouched record is copied");
    }

    #[test]
    fn a_part_whose_bytes_changed_is_refused() {
        let mut bytes = zip(&[("[Content_Types].xml", b"<Types/>", false)]);
        let data_at = {
            let entries = directory(&bytes).expect("directory");
            packed_range(&bytes, &entries[0]).expect("range").start
        };
        bytes[data_at] ^= 0xff;
        let entries = directory(&bytes).expect("directory");
        assert!(read_part(&bytes, &entries[0]).is_err());
    }
}
