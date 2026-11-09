/// The ArtCraft document apps this crate can drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum App {
    GridCraft,
    WordCraft,
    PdfCraft,
}

impl App {
    pub const ALL: [App; 3] = [App::GridCraft, App::WordCraft, App::PdfCraft];

    /// The name the user reads.
    pub fn name(self) -> &'static str {
        match self {
            Self::GridCraft => "GridCraft",
            Self::WordCraft => "WordCraft",
            Self::PdfCraft => "PdfCraft",
        }
    }

    /// The executable's base name; `.exe` is added on Windows.
    pub fn cli(self) -> &'static str {
        match self {
            Self::GridCraft => "gridcraft-cli",
            Self::WordCraft => "wordcraft-cli",
            Self::PdfCraft => "pdfcraft-cli",
        }
    }

    /// The folder under `<datos>/tools/` where the portable build is unzipped.
    pub fn dir(self) -> &'static str {
        match self {
            Self::GridCraft => "gridcraft",
            Self::WordCraft => "wordcraft",
            Self::PdfCraft => "pdfcraft",
        }
    }

    /// The key in `crafting.json` that holds the executable's path.
    pub fn config_key(self) -> &'static str {
        match self {
            Self::GridCraft => "gridcraft_cli",
            Self::WordCraft => "wordcraft_cli",
            Self::PdfCraft => "pdfcraft_cli",
        }
    }

    /// The version these apps were tested with. A different one is noted, not blocked.
    pub fn tested_version(self) -> &'static str {
        match self {
            Self::GridCraft => "0.16.1",
            Self::WordCraft => "0.15.2",
            Self::PdfCraft => "0.18.2",
        }
    }
}
