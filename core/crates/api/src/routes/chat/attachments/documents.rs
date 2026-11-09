//! Documents the chat keeps as the user sent them. The extension picks the
//! format and the mime the host stores; the content must carry that format's
//! signature. OOXML is checked by its zip signature only: the workspace has no
//! zip reader, so `[Content_Types].xml` and the expansion limits are not checked.

use serde::de::IgnoredAny;

use crate::error::ApiError;

#[derive(Clone, Copy)]
enum Format {
    Ooxml,
    Pdf,
    Text,
    Json,
}

/// Extension, the mime the host stores, and the format its content must have.
const ALLOWED: [(&str, &str, Format); 8] = [
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Format::Ooxml,
    ),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Format::Ooxml,
    ),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Format::Ooxml,
    ),
    ("pdf", "application/pdf", Format::Pdf),
    ("csv", "text/csv", Format::Text),
    ("txt", "text/plain", Format::Text),
    ("md", "text/markdown", Format::Text),
    ("json", "application/json", Format::Json),
];

/// `None` when the name is not a listed document, so the image path decides.
/// An error when the name is a document and its content is not that format.
pub(super) fn document_mime(name: &str, bytes: &[u8]) -> Result<Option<&'static str>, ApiError> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let Some(&(_, mime, format)) = ALLOWED
        .iter()
        .find(|(known, _, _)| Some(*known) == extension.as_deref())
    else {
        return Ok(None);
    };
    if matches_format(format, bytes) {
        Ok(Some(mime))
    } else {
        Err(unsupported())
    }
}

fn matches_format(format: Format, bytes: &[u8]) -> bool {
    match format {
        Format::Ooxml => bytes.starts_with(b"PK\x03\x04"),
        Format::Pdf => bytes.starts_with(b"%PDF-"),
        Format::Text => is_text(bytes),
        Format::Json => is_text(bytes) && serde_json::from_slice::<IgnoredAny>(bytes).is_ok(),
    }
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// The message never carries the name or the content: the user may have sent either.
fn unsupported() -> ApiError {
    ApiError::invalid_field(
        "unsupported_document",
        "file",
        "el contenido no coincide con el formato de la extensión",
    )
}

#[cfg(test)]
mod tests {
    use super::document_mime;

    const OOXML: &[u8] = b"PK\x03\x04[Content_Types].xml";
    const PDF: &[u8] = b"%PDF-1.7\n%test\n";

    fn accepted(name: &str, bytes: &[u8]) -> &'static str {
        document_mime(name, bytes)
            .ok()
            .flatten()
            .expect("a listed document in its format")
    }

    fn refused(name: &str, bytes: &[u8]) -> bool {
        document_mime(name, bytes).is_err()
    }

    #[test]
    fn office_files_get_the_mime_the_host_sets() {
        let sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
        let word = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        let slides = "application/vnd.openxmlformats-officedocument.presentationml.presentation";
        assert_eq!(accepted("ventas.xlsx", OOXML), sheet);
        assert_eq!(accepted("carta.docx", OOXML), word);
        assert_eq!(accepted("deck.pptx", OOXML), slides);
    }

    #[test]
    fn pdf_and_text_files_pass_their_signature_or_encoding() {
        assert_eq!(accepted("informe.pdf", PDF), "application/pdf");
        assert_eq!(accepted("datos.csv", b"a,b\n1,2\n"), "text/csv");
        assert_eq!(accepted("nota.txt", "hola ñandú".as_bytes()), "text/plain");
        assert_eq!(accepted("README.md", b"# Titulo\n"), "text/markdown");
        assert_eq!(
            accepted("config.json", br#"{"a":[1,2]}"#),
            "application/json"
        );
    }

    #[test]
    fn the_extension_is_read_in_any_case() {
        assert_eq!(accepted("INFORME.PDF", PDF), "application/pdf");
    }

    #[test]
    fn macro_binary_legacy_and_archive_names_are_not_documents() {
        for name in [
            "libro.xlsm",
            "carta.docm",
            "viejo.xls",
            "viejo.doc",
            "pack.zip",
            "run.exe",
        ] {
            assert!(matches!(document_mime(name, OOXML), Ok(None)), "{name}");
        }
    }

    #[test]
    fn an_image_name_is_left_to_the_image_path() {
        assert!(matches!(
            document_mime("captura.png", b"\x89PNG\r\n"),
            Ok(None)
        ));
        assert!(matches!(document_mime("sin_extension", PDF), Ok(None)));
    }

    #[test]
    fn content_that_does_not_match_its_format_is_refused() {
        assert!(refused("ventas.xlsx", b"no es un zip"));
        assert!(refused("informe.pdf", b"<html>"));
        assert!(refused("nota.txt", b"texto\0oculto"));
        assert!(refused("datos.csv", &[0xff, 0xfe, 0x00]));
        assert!(refused("config.json", b"{ no json"));
    }
}
