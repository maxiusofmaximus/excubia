use crate::CraftError;

/// The attachment types the chat can read. Macro-enabled and legacy Office
/// formats are left out on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Xlsx,
    Docx,
    Pdf,
    Csv,
    Txt,
    Md,
    Json,
}

impl Kind {
    /// The kind of a chat attachment. The extension picks it, and the mime
    /// must agree, so a renamed file is refused.
    pub fn of(name: &str, mime: &str) -> Result<Self, CraftError> {
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        let kind = match extension.as_deref() {
            Some("xlsx") => Self::Xlsx,
            Some("docx") => Self::Docx,
            Some("pdf") => Self::Pdf,
            Some("csv") => Self::Csv,
            Some("txt") => Self::Txt,
            Some("md") => Self::Md,
            Some("json") => Self::Json,
            _ => {
                return Err(CraftError::Unsupported(
                    "este tipo de archivo no tiene lector",
                ))
            }
        };
        if kind.mime() == mime {
            Ok(kind)
        } else {
            Err(CraftError::BadDocument(
                "el tipo no coincide con la extensión",
            ))
        }
    }

    /// The extension the working copy must carry, since the CLIs decide by it.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Docx => "docx",
            Self::Pdf => "pdf",
            Self::Csv => "csv",
            Self::Txt => "txt",
            Self::Md => "md",
            Self::Json => "json",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Pdf => "application/pdf",
            Self::Csv => "text/csv",
            Self::Txt => "text/plain",
            Self::Md => "text/markdown",
            Self::Json => "application/json",
        }
    }

    /// Office packages are ZIP files and get the directory check before any CLI sees them.
    pub fn is_ooxml(self) -> bool {
        matches!(self, Self::Xlsx | Self::Docx)
    }
}
