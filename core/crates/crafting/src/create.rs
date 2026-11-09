//! Making a new document from text (DOC8). Excel, Word and PDF are built by their CLIs
//! from a private copy of the text; CSV, text, Markdown and JSON are kept as written.
//! Nothing here touches the chat: the host stores the bytes this returns.

use crate::driver::{Driver, RENDER_TIMEOUT};
use crate::workspace::{Workspace, MAX_INPUT_BYTES};
use crate::{ooxml, App, CraftError, Kind};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

/// Most cells a sheet made from CSV may have.
pub const MAX_CELLS: usize = 50_000;

impl Driver<'_> {
    /// The bytes of a new document of `kind` made from `content`.
    pub fn create_file(&self, kind: Kind, content: &str) -> Result<Vec<u8>, CraftError> {
        if content.trim().is_empty() {
            return Err(CraftError::Invalid("el contenido está vacío"));
        }
        match kind {
            Kind::Xlsx => {
                check_cells(content)?;
                self.build(App::GridCraft, kind, "entrada.csv", content)
            }
            Kind::Docx | Kind::Pdf => self.build(App::WordCraft, kind, "entrada.md", content),
            Kind::Json => {
                serde_json::from_str::<serde::de::IgnoredAny>(content)
                    .map_err(|_| CraftError::Invalid("el JSON no es válido"))?;
                Ok(content.as_bytes().to_vec())
            }
            Kind::Csv | Kind::Txt | Kind::Md => Ok(content.as_bytes().to_vec()),
        }
    }

    /// Writes `content` as `input` in a private folder, converts it with `app`, and
    /// returns the output once it has the kind's format and size.
    fn build(
        &self,
        app: App,
        kind: Kind,
        input: &str,
        content: &str,
    ) -> Result<Vec<u8>, CraftError> {
        let ws = Workspace::create(&self.base)?;
        let source = ws.join(input);
        fs::write(&source, content)?;
        let out = ws.join(&format!("salida.{}", kind.extension()));
        let convert: Vec<OsString> =
            vec![OsString::from("convert"), source.into(), out.clone().into()];
        self.invoke(app, convert, ws.path(), RENDER_TIMEOUT)?;
        let bytes = read_output(&out)?;
        check_output(kind, &bytes)?;
        Ok(bytes)
    }
}

/// The cells of a CSV text: each unquoted separator is one cell past the first in its
/// row, and each row ends with a line break (the last one may be missing).
fn cell_count(csv: &str) -> usize {
    let (mut commas, mut rows, mut quoted) = (0usize, 0usize, false);
    for ch in csv.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => commas += 1,
            '\n' if !quoted => rows += 1,
            _ => {}
        }
    }
    if !csv.ends_with('\n') {
        rows += 1;
    }
    commas + rows
}

fn check_cells(csv: &str) -> Result<(), CraftError> {
    if cell_count(csv) > MAX_CELLS {
        Err(CraftError::Invalid("la hoja pasa de 50 000 celdas"))
    } else {
        Ok(())
    }
}

/// The CLI's file, read only when it exists and fits the size limit.
fn read_output(path: &Path) -> Result<Vec<u8>, CraftError> {
    let size = fs::metadata(path)
        .map_err(|_| CraftError::BadOutput("no se creó el archivo"))?
        .len();
    if size > MAX_INPUT_BYTES {
        return Err(CraftError::BadOutput("el archivo pasa de 10 MiB"));
    }
    Ok(fs::read(path)?)
}

fn check_output(kind: Kind, bytes: &[u8]) -> Result<(), CraftError> {
    if kind.is_ooxml() {
        return ooxml::check(bytes)
            .map_err(|_| CraftError::BadOutput("la salida no es un documento válido"));
    }
    if bytes.starts_with(b"%PDF-") {
        Ok(())
    } else {
        Err(CraftError::BadOutput("la salida no es PDF"))
    }
}
