//! A PNG preview of one page: PdfCraft renders PDF, WordCraft renders Word, and a
//! spreadsheet goes to PDF through GridCraft first.

use crate::driver::{check_page, Document, Driver, RENDER_TIMEOUT, TEXT_TIMEOUT};
use crate::read::page_count;
use crate::workspace::Workspace;
use crate::{App, CraftError, Kind};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

/// Largest preview the chat accepts.
const PNG_LIMIT: u64 = 8 * 1024 * 1024;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// One page as PNG, and the document's page count when it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub png: Vec<u8>,
    /// `None` for Word, which no CLI here counts, and when PdfCraft cannot say.
    pub pages: Option<u32>,
}

impl Driver<'_> {
    pub fn render_png(&self, doc: &Document, page: u32) -> Result<Preview, CraftError> {
        let kind = Kind::of(&doc.name, &doc.mime)?;
        check_page(page)?;
        let ws = Workspace::create(&self.base)?;
        let input = ws.stage(doc, kind)?;
        let png = ws.join("page.png");
        // The PDF whose pages are counted, when there is one. Counted only after the
        // PNG passes its checks, so a refused image costs no second CLI call.
        let counted = match kind {
            Kind::Pdf => {
                self.render_pdf(&ws, &input, page, &png)?;
                Some(input)
            }
            Kind::Docx => {
                let command: Vec<OsString> = vec![
                    OsString::from("render"),
                    input.into(),
                    png.clone().into(),
                    OsString::from("--page"),
                    OsString::from(page.to_string()),
                ];
                self.invoke(App::WordCraft, command, ws.path(), RENDER_TIMEOUT)?;
                None
            }
            Kind::Xlsx => {
                let pdf = ws.join("preview.pdf");
                let convert: Vec<OsString> =
                    vec![OsString::from("convert"), input.into(), pdf.clone().into()];
                self.invoke(App::GridCraft, convert, ws.path(), RENDER_TIMEOUT)?;
                self.render_pdf(&ws, &pdf, page, &png)?;
                Some(pdf)
            }
            Kind::Csv | Kind::Txt | Kind::Md | Kind::Json => {
                return Err(CraftError::Unsupported(
                    "solo se previsualizan PDF, Word y hojas de cálculo",
                ));
            }
        };
        let image = read_png(&png)?;
        let pages = counted.and_then(|pdf| self.pdf_pages(&ws, &pdf));
        Ok(Preview { png: image, pages })
    }

    /// The page count PdfCraft's text gives. A cut text would count wrong, so it gives none.
    fn pdf_pages(&self, ws: &Workspace, pdf: &Path) -> Option<u32> {
        let text: Vec<OsString> = vec![OsString::from("text"), pdf.into()];
        let out = self
            .invoke(App::PdfCraft, text, ws.path(), TEXT_TIMEOUT)
            .ok()?;
        if out.truncated {
            return None;
        }
        u32::try_from(page_count(&out.stdout)).ok()
    }

    fn render_pdf(
        &self,
        ws: &Workspace,
        pdf: &Path,
        page: u32,
        png: &Path,
    ) -> Result<(), CraftError> {
        let command: Vec<OsString> = vec![
            OsString::from("render"),
            pdf.into(),
            OsString::from("--page"),
            OsString::from(page.to_string()),
            OsString::from("--out"),
            png.into(),
        ];
        match self.invoke(App::PdfCraft, command, ws.path(), RENDER_TIMEOUT) {
            Ok(_) => Ok(()),
            Err(err @ CraftError::Failed(..)) => {
                Err(self.explain_render_failure(ws, pdf, page, err))
            }
            Err(err) => Err(err),
        }
    }

    /// A page past the end is reported as such; any other failure keeps its own error.
    fn explain_render_failure(
        &self,
        ws: &Workspace,
        pdf: &Path,
        page: u32,
        err: CraftError,
    ) -> CraftError {
        let text = vec![OsString::from("text"), pdf.into()];
        match self.invoke(App::PdfCraft, text, ws.path(), TEXT_TIMEOUT) {
            Ok(out) if page as usize > page_count(&out.stdout) => CraftError::PageOutOfRange(page),
            _ => err,
        }
    }
}

/// The image must exist, fit the limit and start with the PNG signature.
fn read_png(path: &Path) -> Result<Vec<u8>, CraftError> {
    let size = fs::metadata(path)
        .map_err(|_| CraftError::BadOutput("no se generó la imagen"))?
        .len();
    if size > PNG_LIMIT {
        return Err(CraftError::BadOutput("la imagen pasa de 8 MiB"));
    }
    let bytes = fs::read(path)?;
    if bytes.starts_with(&PNG_SIGNATURE) {
        Ok(bytes)
    } else {
        Err(CraftError::BadOutput("la salida no es PNG"))
    }
}
