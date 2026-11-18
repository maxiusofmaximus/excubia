//! The barriers of `mail.attachment_read` (TASK-ATT1), before any CLI sees a file that a
//! received mail carried. The type is decided by the first bytes, never by the name or
//! by the mime the sender gave. Macro-enabled and encrypted files and anything unknown
//! are refused. The working copy lives in a quarantine folder of the host, one folder
//! per read, removed when the read ends whatever its outcome.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use excubia_chat::{ToolError, ToolErrorKind};
use excubia_crafting::{ooxml, Kind};

const PDF_MAGIC: &[u8] = b"%PDF-";
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
/// The header of the old Office formats and of encrypted Office files.
const OLE_MAGIC: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
/// Where Office macros live. The name is stored uncompressed in the ZIP directory.
const MACRO_PART: &[u8] = b"vbaProject.bin";
/// A PDF with encryption has this key in its trailer.
const PDF_ENCRYPT: &[u8] = b"/Encrypt";

/// The kind of an attachment from its first bytes, or the refusal the reader answers with.
pub fn sniff(bytes: &[u8], name: &str) -> Result<Kind, ToolError> {
    if has_macro_extension(name) {
        return Err(refused(
            "document_has_macros",
            "El documento tiene macros: no se lee",
        ));
    }
    if bytes.starts_with(PDF_MAGIC) {
        if contains(bytes, PDF_ENCRYPT) {
            return Err(refused("document_encrypted", ENCRYPTED));
        }
        return Ok(Kind::Pdf);
    }
    if bytes.starts_with(ZIP_MAGIC) {
        ooxml::check(bytes).map_err(|_| refused("bad_document", BAD))?;
        if contains(bytes, MACRO_PART) {
            return Err(refused(
                "document_has_macros",
                "El documento tiene macros: no se lee",
            ));
        }
        // shortcut: the parts are found by a name search over the ZIP; a package that embeds the other kind uncompressed may be misread, and the CLI then fails closed.
        if contains(bytes, b"word/document.xml") {
            return Ok(Kind::Docx);
        }
        if contains(bytes, b"xl/workbook.xml") {
            return Ok(Kind::Xlsx);
        }
        return Err(unsupported());
    }
    if bytes.starts_with(OLE_MAGIC) {
        // Legacy Office or an encrypted Office file: neither is read.
        return Err(unsupported());
    }
    if !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() {
        return Ok(Kind::Txt);
    }
    Err(unsupported())
}

const ENCRYPTED: &str = "El documento está cifrado: no se puede leer";
const BAD: &str = "El documento no se puede leer: no es un archivo válido";

fn unsupported() -> ToolError {
    refused(
        "unsupported_document",
        "Este tipo de archivo no se puede leer: solo PDF, Word, Excel, CSV o texto",
    )
}

fn refused(code: &'static str, message: &str) -> ToolError {
    ToolError {
        kind: ToolErrorKind::Invalid,
        code,
        field: Some("attachment_id"),
        message: message.to_string(),
    }
}

fn has_macro_extension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".docm") || lower.ends_with(".xlsm")
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

static READS: AtomicU64 = AtomicU64::new(0);

/// One read's folder in the quarantine. It is removed when dropped, so the bytes do not
/// outlive the read, on success or on failure.
pub struct Quarantine {
    dir: PathBuf,
}

impl Quarantine {
    /// A new, empty folder under `root`, with the owner's access only where the platform has modes.
    pub fn open(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        restrict(root, 0o700)?;
        let n = READS.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("read-{}-{n}", std::process::id()));
        fs::create_dir(&dir)?;
        restrict(&dir, 0o700)?;
        Ok(Self { dir })
    }

    /// Writes the bytes as `name` in this read's folder and returns the path.
    pub fn write(&self, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let path = self.dir.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        restrict(&path, 0o600)?;
        file.write_all(bytes)?;
        Ok(path)
    }
}

impl Drop for Quarantine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

/// Windows has no mode bits: the folder sits under the user's data directory and keeps its ACL.
#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ZIP directory with the named entries, laid out as the end-of-directory check reads it.
    fn zip(entries: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for name in entries {
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([0u8; 22]);
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());

            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([0u8; 20]);
            central.extend(0u32.to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 6]);
            central.extend([0u8; 6]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let directory = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(directory.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn package(parts: &[&str]) -> Vec<u8> {
        let mut entries = vec!["[Content_Types].xml"];
        entries.extend(parts);
        zip(&entries)
    }

    fn code(result: Result<Kind, ToolError>) -> &'static str {
        result.expect_err("refused").code
    }

    #[test]
    fn the_type_comes_from_the_first_bytes_not_from_the_name() {
        assert_eq!(sniff(b"%PDF-1.7\nbody", "factura.pdf").unwrap(), Kind::Pdf);
        assert_eq!(
            sniff(&package(&["word/document.xml"]), "carta.pdf").unwrap(),
            Kind::Docx
        );
        assert_eq!(
            sniff(&package(&["xl/workbook.xml"]), "datos.txt").unwrap(),
            Kind::Xlsx
        );
        assert_eq!(sniff("a,b\n1,2\n".as_bytes(), "x.bin").unwrap(), Kind::Txt);
        // A program renamed to .pdf is an unknown type, whatever its name says.
        assert_eq!(
            code(sniff(b"MZ\x90\x00\x03\x00\x00\x00", "factura.pdf")),
            "unsupported_document"
        );
        assert_eq!(
            code(sniff(b"\x89PNG\r\n\x1a\n", "foto.pdf")),
            "unsupported_document"
        );
    }

    #[test]
    fn a_text_with_a_zero_byte_or_bad_utf8_is_not_text() {
        assert_eq!(code(sniff(b"ab\0cd", "a.txt")), "unsupported_document");
        assert_eq!(
            code(sniff(&[0xff, 0xfe, 0x41], "a.txt")),
            "unsupported_document"
        );
    }

    #[test]
    fn a_macro_package_is_refused_by_its_part_or_its_name() {
        let with_macros = package(&["word/document.xml", "word/vbaProject.bin"]);
        assert_eq!(
            code(sniff(&with_macros, "contrato.docx")),
            "document_has_macros"
        );
        let plain = package(&["word/document.xml"]);
        assert_eq!(code(sniff(&plain, "contrato.DOCM")), "document_has_macros");
        assert_eq!(code(sniff(&plain, "tabla.xlsm")), "document_has_macros");
        assert_eq!(sniff(&plain, "contrato.docx").unwrap(), Kind::Docx);
    }

    #[test]
    fn an_encrypted_or_legacy_file_is_not_read() {
        assert_eq!(
            code(sniff(b"%PDF-1.6\n<< /Encrypt 5 0 R >>", "a.pdf")),
            "document_encrypted"
        );
        let legacy = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1rest of a compound file";
        assert_eq!(code(sniff(legacy, "viejo.doc")), "unsupported_document");
    }

    #[test]
    fn a_zip_that_is_not_an_office_package_is_refused() {
        let plain_zip = zip(&["notas.txt"]);
        assert_eq!(code(sniff(&plain_zip, "a.zip")), "bad_document");
        let broken = b"PK\x03\x04 nothing else here";
        assert_eq!(code(sniff(broken, "a.docx")), "bad_document");
        let no_content_types = zip(&["word/document.xml"]);
        assert_eq!(code(sniff(&no_content_types, "a.docx")), "bad_document");
    }

    #[test]
    fn the_quarantine_is_removed_when_its_read_ends() {
        let root =
            std::env::temp_dir().join(format!("excubia-quarantine-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder;
        {
            let quarantine = Quarantine::open(&root).expect("open");
            let path = quarantine.write("factura.pdf", b"%PDF-").expect("write");
            assert_eq!(fs::read(&path).expect("read back"), b"%PDF-");
            folder = path.parent().expect("folder").to_path_buf();
            assert!(folder.is_dir());
        }
        assert!(!folder.exists(), "the read's folder is gone");
        assert_eq!(
            fs::read_dir(&root).expect("root").count(),
            0,
            "nothing is left behind"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn each_read_gets_its_own_folder() {
        let root =
            std::env::temp_dir().join(format!("excubia-quarantine-own-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let first = Quarantine::open(&root).expect("first");
        let second = Quarantine::open(&root).expect("second");
        assert_ne!(first.dir, second.dir);
        drop(first);
        drop(second);
        assert_eq!(fs::read_dir(&root).expect("root").count(), 0);
        let _ = fs::remove_dir_all(&root);
    }
}
