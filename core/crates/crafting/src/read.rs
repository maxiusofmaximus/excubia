//! Reading an attachment as text: GridCraft for sheets, WordCraft for Word,
//! PdfCraft for PDF, and plain UTF-8 for the rest.

use crate::driver::{check_page, Document, Driver, TEXT_TIMEOUT};
use crate::runner::{Captured, MAX_OUTPUT_BYTES};
use crate::workspace::Workspace;
use crate::{App, CraftError, Kind};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

/// Cells read from a sheet when the caller gives no range: the first 200 rows.
pub const DEFAULT_RANGE: &str = "A1:ZZ200";

/// What to read. Each option applies only to the kind named beside it and is
/// ignored elsewhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOpts {
    /// xlsx: the sheet's name. Default: the active sheet, else the first.
    pub sheet: Option<String>,
    /// xlsx: a cell range such as `B2:D20`. Default: `DEFAULT_RANGE`.
    pub range: Option<String>,
    /// pdf: a one-based page. Default: the whole document.
    pub page: Option<u32>,
}

/// The text of an attachment, with the sheets or pages it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOut {
    pub text: String,
    pub truncated: bool,
    pub pages_or_sheets: Vec<String>,
}

impl Driver<'_> {
    pub fn read_text(&self, doc: &Document, opts: &ReadOpts) -> Result<ReadOut, CraftError> {
        let kind = Kind::of(&doc.name, &doc.mime)?;
        if let Some(page) = opts.page {
            check_page(page)?;
        }
        let ws = Workspace::create(&self.base)?;
        let input = ws.stage(doc, kind)?;
        match kind {
            Kind::Xlsx => self.read_grid(&ws, &input, opts),
            Kind::Docx => {
                let command: Vec<OsString> = vec![OsString::from("text"), input.into()];
                let out = self.invoke(App::WordCraft, command, ws.path(), TEXT_TIMEOUT)?;
                Ok(text_out(out, Vec::new()))
            }
            Kind::Pdf => self.read_pdf(&ws, &input, opts.page),
            Kind::Csv | Kind::Txt | Kind::Md | Kind::Json => read_plain(&input),
        }
    }

    /// The sheet list comes from the plain `info` (one line per sheet); the cells from
    /// `cat`, as CSV. `info --json` is not used: on a workbook with many merged cells it
    /// runs to megabytes and the output cap cuts it.
    fn read_grid(
        &self,
        ws: &Workspace,
        input: &Path,
        opts: &ReadOpts,
    ) -> Result<ReadOut, CraftError> {
        let info: Vec<OsString> = vec![OsString::from("info"), input.into()];
        let out = self.invoke(App::GridCraft, info, ws.path(), TEXT_TIMEOUT)?;
        let (sheets, active) = sheet_lines(&String::from_utf8_lossy(&out.stdout));
        let sheet = match &opts.sheet {
            Some(name) => name.clone(),
            None => active
                .or_else(|| sheets.first().cloned())
                .ok_or(CraftError::BadDocument("la hoja de cálculo no tiene hojas"))?,
        };
        let range = opts
            .range
            .clone()
            .unwrap_or_else(|| DEFAULT_RANGE.to_string());
        let cat: Vec<OsString> = vec![
            OsString::from("cat"),
            input.into(),
            OsString::from("--sheet"),
            sheet.into(),
            OsString::from("--range"),
            range.into(),
            OsString::from("--csv"),
        ];
        let out = self.invoke(App::GridCraft, cat, ws.path(), TEXT_TIMEOUT)?;
        Ok(text_out(out, sheets))
    }

    fn read_pdf(
        &self,
        ws: &Workspace,
        input: &Path,
        page: Option<u32>,
    ) -> Result<ReadOut, CraftError> {
        let mut command: Vec<OsString> = vec![OsString::from("text"), input.into()];
        if let Some(page) = page {
            command.push(OsString::from("--page"));
            command.push(OsString::from(page.to_string()));
        }
        let out = self.invoke(App::PdfCraft, command, ws.path(), TEXT_TIMEOUT)?;
        let pages = match page {
            Some(page) => vec![page.to_string()],
            None => (1..=page_count(&out.stdout))
                .map(|number| number.to_string())
                .collect(),
        };
        Ok(text_out(out, pages))
    }
}

/// A chat file that is already text: read whole, checked as UTF-8, and cut at the output limit.
/// The sheet names of GridCraft's plain `info`, and the active one (`* `). After the file
/// name, each line is `  Name: used A1:B2, N cells` or `* Name (Hidden): …`.
pub(crate) fn sheet_lines(info: &str) -> (Vec<String>, Option<String>) {
    let mut sheets = Vec::new();
    let mut active = None;
    for line in info.lines().skip(1) {
        let (is_active, rest) = match line.strip_prefix("* ") {
            Some(rest) => (true, rest),
            None => match line.strip_prefix("  ") {
                Some(rest) => (false, rest),
                None => continue,
            },
        };
        let Some((name, _)) = rest.rsplit_once(": ") else {
            continue;
        };
        let name = name.trim_end_matches(" (Hidden)").to_string();
        if is_active {
            active = Some(name.clone());
        }
        sheets.push(name);
    }
    (sheets, active)
}

fn read_plain(input: &Path) -> Result<ReadOut, CraftError> {
    let bytes = fs::read(input)?;
    let whole =
        std::str::from_utf8(&bytes).map_err(|_| CraftError::BadDocument("no es texto UTF-8"))?;
    let mut end = whole.len().min(MAX_OUTPUT_BYTES);
    while !whole.is_char_boundary(end) {
        end -= 1;
    }
    Ok(ReadOut {
        text: whole[..end].to_string(),
        truncated: end < whole.len(),
        pages_or_sheets: Vec::new(),
    })
}

/// PdfCraft's text puts one form feed between pages, so the pages are the feeds plus one.
pub(crate) fn page_count(text: &[u8]) -> usize {
    text.iter().filter(|&&byte| byte == b'\x0c').count() + 1
}

fn text_out(out: Captured, pages_or_sheets: Vec<String>) -> ReadOut {
    ReadOut {
        text: String::from_utf8_lossy(&out.stdout).into_owned(),
        truncated: out.truncated,
        pages_or_sheets,
    }
}
