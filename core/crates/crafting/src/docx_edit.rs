//! Edits a `.docx` without touching the rest of the package: `word/document.xml` is the
//! only part rewritten, every other entry is copied byte for byte (see `ooxml`).
//!
//! The XML is scanned as tags, not parsed as a tree. Tables are numbered at the top level
//! and their columns follow the cells as written: a merged cell (`gridSpan`) counts as one.

use std::ops::Range;

use crate::ooxml;
use crate::CraftError;

const DOCUMENT: &[u8] = b"word/document.xml";

/// One edit. `replace` looks through the whole document and joins text that Word
/// split across runs. `Cell` numbers tables, rows and columns from 1.
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    Replace {
        find: &'a str,
        replace: &'a str,
    },
    Cell {
        table: usize,
        row: usize,
        col: usize,
        text: &'a str,
    },
}

/// The edited package. Refused with a clear message when a change names something the
/// document does not have, so the caller never delivers a half-applied copy.
pub fn edit_docx(bytes: &[u8], changes: &[Change<'_>]) -> Result<Vec<u8>, CraftError> {
    ooxml::check(bytes)?;
    let entries = ooxml::directory(bytes)?;
    let entry = entries
        .iter()
        .find(|entry| entry.name == DOCUMENT)
        .ok_or(CraftError::BadDocument("falta word/document.xml"))?;
    let mut xml = String::from_utf8(ooxml::read_part(bytes, entry)?)
        .map_err(|_| CraftError::BadDocument("word/document.xml no es UTF-8"))?;
    for change in changes {
        xml = match *change {
            Change::Replace { find, replace } => {
                if find.is_empty() {
                    return Err(CraftError::Invalid("el texto a buscar está vacío"));
                }
                let (edited, count) = replace_text(&xml, find, replace);
                if count == 0 {
                    return Err(CraftError::Target(format!(
                        "no encuentro «{find}» en el documento"
                    )));
                }
                edited
            }
            Change::Cell {
                table,
                row,
                col,
                text,
            } => set_cell(&xml, table, row, col, text)?,
        };
    }
    ooxml::replace_part(bytes, DOCUMENT, xml.as_bytes())
}

/// Replaces every match in every paragraph, tables included. Returns the new XML and
/// how many matches it changed.
fn replace_text(xml: &str, find: &str, replace: &str) -> (String, usize) {
    let mut out = String::with_capacity(xml.len());
    let mut pos = 0;
    let mut total = 0;
    for paragraph in outermost(xml, 0..xml.len(), "w:p") {
        let (edited, count) = replace_in_paragraph(xml, paragraph.clone(), find, replace);
        if count > 0 {
            out.push_str(&xml[pos..paragraph.start]);
            out.push_str(&edited);
            pos = paragraph.end;
            total += count;
        }
    }
    out.push_str(&xml[pos..]);
    (out, total)
}

fn replace_in_paragraph(
    xml: &str,
    paragraph: Range<usize>,
    find: &str,
    replace: &str,
) -> (String, usize) {
    let spans = text_spans(xml, paragraph.clone());
    let mut pieces: Vec<String> = spans
        .iter()
        .map(|span| unescape(&xml[span.content.clone()]))
        .collect();
    let count = replace_in_pieces(&mut pieces, find, replace);
    if count == 0 {
        return (String::new(), 0);
    }
    (write_spans(xml, paragraph, &spans, &pieces), count)
}

/// Sets the text of one cell. A cell with text keeps its first paragraph and the format
/// of its first run; the other paragraphs of the cell go. An empty cell gets a run that
/// copies the paragraph mark's format.
fn set_cell(
    xml: &str,
    table: usize,
    row: usize,
    col: usize,
    text: &str,
) -> Result<String, CraftError> {
    if table == 0 || row == 0 || col == 0 {
        return Err(CraftError::Target(
            "las tablas, filas y columnas empiezan en 1".to_string(),
        ));
    }
    let tables = outermost(xml, 0..xml.len(), "w:tbl");
    let table_at = pick(&tables, table, || {
        format!("no hay tabla {table} (el documento tiene {})", tables.len())
    })?;
    let rows = direct(xml, inner(xml, &table_at, "w:tbl"), "w:tr");
    let row_at = pick(&rows, row, || {
        format!("no hay fila {row} en la tabla {table} (tiene {})", rows.len())
    })?;
    let cells = direct(xml, inner(xml, &row_at, "w:tr"), "w:tc");
    let cell_at = pick(&cells, col, || {
        format!("no hay columna {col} en la fila {row} (tiene {})", cells.len())
    })?;
    let edited = cell_with_text(xml, &cell_at, text)?;
    Ok(format!(
        "{}{}{}",
        &xml[..cell_at.start],
        edited,
        &xml[cell_at.end..]
    ))
}

fn pick(
    items: &[Range<usize>],
    index: usize,
    missing: impl FnOnce() -> String,
) -> Result<Range<usize>, CraftError> {
    items
        .get(index - 1)
        .cloned()
        .ok_or_else(|| CraftError::Target(missing()))
}

fn cell_with_text(xml: &str, cell: &Range<usize>, text: &str) -> Result<String, CraftError> {
    let paragraphs = direct(xml, inner(xml, cell, "w:tc"), "w:p");
    let first = paragraphs
        .first()
        .cloned()
        .ok_or_else(|| CraftError::Target("una celda no tiene párrafo".to_string()))?;
    let first_edited = paragraph_with_text(xml, first.clone(), text);
    let content = inner(xml, cell, "w:tc");
    let mut out = String::new();
    out.push_str(&xml[cell.start..content.start]);
    out.push_str(&xml[content.start..first.start]);
    out.push_str(&first_edited);
    let mut pos = first.end;
    for paragraph in &paragraphs[1..] {
        out.push_str(&xml[pos..paragraph.start]);
        pos = paragraph.end;
    }
    out.push_str(&xml[pos..cell.end]);
    Ok(out)
}

fn paragraph_with_text(xml: &str, paragraph: Range<usize>, text: &str) -> String {
    let spans = text_spans(xml, paragraph.clone());
    if !spans.is_empty() {
        let mut pieces = vec![String::new(); spans.len()];
        pieces[0] = text.to_string();
        return write_spans(xml, paragraph, &spans, &pieces);
    }
    let run = format!(
        "<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        mark_props(&xml[paragraph.clone()]),
        escape(text)
    );
    let whole = &xml[paragraph.clone()];
    if let Some(open) = whole.strip_suffix("/>") {
        return format!("{open}>{run}</w:p>");
    }
    let close = paragraph.end - "</w:p>".len();
    format!("{}{run}{}", &xml[paragraph.start..close], &xml[close..paragraph.end])
}

/// The run properties a paragraph mark carries, `<w:rPr>…</w:rPr>`, or nothing.
fn mark_props(paragraph: &str) -> &str {
    let Some(start) = paragraph.find("<w:pPr>") else {
        return "";
    };
    let Some(end) = paragraph[start..].find("</w:pPr>") else {
        return "";
    };
    let props = &paragraph[start..start + end];
    let Some(at) = props.find("<w:rPr>") else {
        return "";
    };
    match props[at..].find("</w:rPr>") {
        Some(end) => &props[at..at + end + "</w:rPr>".len()],
        None => "",
    }
}

/// The text pieces of a paragraph, one per `w:t` in order. Pieces replace each other
/// without moving the match boundaries: the first piece a match touches gets the
/// replacement, the others lose their part of it.
fn replace_in_pieces(pieces: &mut [String], find: &str, replace: &str) -> usize {
    let joined: String = pieces.concat();
    let matches: Vec<(usize, usize)> = joined
        .match_indices(find)
        .map(|(at, found)| (at, at + found.len()))
        .collect();
    for &(start, end) in matches.iter().rev() {
        splice(pieces, start, end, replace);
    }
    matches.len()
}

fn splice(pieces: &mut [String], start: usize, end: usize, with: &str) {
    let mut offset = 0;
    let mut placed = false;
    for piece in pieces.iter_mut() {
        let (from_piece, to_piece) = (offset, offset + piece.len());
        offset = to_piece;
        let (from, to) = (start.max(from_piece), end.min(to_piece));
        if from >= to {
            continue;
        }
        let middle = if placed { "" } else { with };
        placed = true;
        let edited = format!(
            "{}{}{}",
            &piece[..from - from_piece],
            middle,
            &piece[to - from_piece..]
        );
        *piece = edited;
    }
}

/// Writes the pieces back into the paragraph's `w:t` elements; everything else in the
/// paragraph is copied as it was.
fn write_spans(xml: &str, paragraph: Range<usize>, spans: &[Span], pieces: &[String]) -> String {
    let mut out = String::new();
    let mut pos = paragraph.start;
    for (span, piece) in spans.iter().zip(pieces) {
        out.push_str(&xml[pos..span.open.start]);
        let open = &xml[span.open.clone()];
        let spaced = piece.starts_with(char::is_whitespace) || piece.ends_with(char::is_whitespace);
        if spaced && !open.contains("xml:space") {
            out.push_str(&open[..open.len() - 1]);
            out.push_str(" xml:space=\"preserve\">");
        } else {
            out.push_str(open);
        }
        out.push_str(&escape(piece));
        out.push_str("</w:t>");
        pos = span.content.end + "</w:t>".len();
    }
    out.push_str(&xml[pos..paragraph.end]);
    out
}

struct Span {
    open: Range<usize>,
    content: Range<usize>,
}

fn text_spans(xml: &str, paragraph: Range<usize>) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut open: Option<Range<usize>> = None;
    for tag in tags(xml, paragraph) {
        match (tag.kind, tag.name) {
            (Kind::Open, "w:t") => open = Some(tag.start..tag.end),
            (Kind::Close, "w:t") => {
                if let Some(open) = open.take() {
                    spans.push(Span {
                        content: open.end..tag.start,
                        open,
                    });
                }
            }
            _ => {}
        }
    }
    spans
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Open,
    Close,
    Empty,
}

#[derive(Debug, Clone, Copy)]
struct Tag<'a> {
    start: usize,
    end: usize,
    name: &'a str,
    kind: Kind,
}

/// Every tag in `range` of the XML, in order. Declarations and comments are skipped.
fn tags(xml: &str, range: Range<usize>) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let mut at = range.start;
    while let Some(rel) = xml[at..range.end].find('<') {
        let start = at + rel;
        let Some(close) = xml[start..range.end].find('>') else {
            break;
        };
        let end = start + close + 1;
        at = end;
        let inner = &xml[start + 1..end - 1];
        if matches!(inner.as_bytes().first(), Some(b'?' | b'!')) {
            continue;
        }
        let (kind, body) = match inner.strip_prefix('/') {
            Some(body) => (Kind::Close, body),
            None if inner.ends_with('/') => (Kind::Empty, &inner[..inner.len() - 1]),
            None => (Kind::Open, inner),
        };
        let name_end = body
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(body.len());
        out.push(Tag {
            start,
            end,
            name: &body[..name_end],
            kind,
        });
    }
    out
}

/// The elements named `name` that no other element of that name contains.
fn outermost(xml: &str, range: Range<usize>, name: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for tag in tags(xml, range) {
        if tag.name != name {
            continue;
        }
        match tag.kind {
            Kind::Open => {
                if depth == 0 {
                    start = tag.start;
                }
                depth += 1;
            }
            Kind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    out.push(start..tag.end);
                }
            }
            Kind::Empty => {
                if depth == 0 {
                    out.push(tag.start..tag.end);
                }
            }
        }
    }
    out
}

/// The elements named `name` that are direct children of the element whose content is
/// `content`.
fn direct(xml: &str, content: Range<usize>, name: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut child = "";
    for tag in tags(xml, content) {
        match tag.kind {
            Kind::Open => {
                if depth == 0 {
                    start = tag.start;
                    child = tag.name;
                }
                depth += 1;
            }
            Kind::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 && child == name {
                    out.push(start..tag.end);
                }
            }
            Kind::Empty => {
                if depth == 0 && tag.name == name {
                    out.push(tag.start..tag.end);
                }
            }
        }
    }
    out
}

/// The content of an element between its tags. A self-closing one has none.
fn inner(xml: &str, element: &Range<usize>, name: &str) -> Range<usize> {
    let text = &xml[element.clone()];
    if text.ends_with("/>") {
        return element.end..element.end;
    }
    let open_end = element.start + text.find('>').map_or(0, |at| at + 1);
    open_end..element.end - (name.len() + 3)
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(semi) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=semi]),
        }
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ooxml::fixtures::zip;

    const CONTENT_TYPES: &[u8] = b"<Types/>";

    fn docx(document: &str) -> Vec<u8> {
        zip(&[
            ("[Content_Types].xml", CONTENT_TYPES, true),
            ("word/styles.xml", b"<w:styles>estilo</w:styles>", false),
            ("word/document.xml", document.as_bytes(), true),
        ])
    }

    fn document_xml(bytes: &[u8]) -> String {
        let entries = ooxml::directory(bytes).expect("directory");
        let entry = entries
            .iter()
            .find(|entry| entry.name == DOCUMENT)
            .expect("document");
        String::from_utf8(ooxml::read_part(bytes, entry).expect("read")).expect("utf8")
    }

    /// The visible text of the document: every `w:t`, unescaped, paragraphs on their own line.
    fn text_of(bytes: &[u8]) -> String {
        let xml = document_xml(bytes);
        outermost(&xml, 0..xml.len(), "w:p")
            .into_iter()
            .map(|paragraph| {
                text_spans(&xml, paragraph)
                    .iter()
                    .map(|span| unescape(&xml[span.content.clone()]))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    const TABLE: &str = "<w:tbl><w:tblPr/><w:tr>\
        <w:tc><w:p><w:r><w:t>Nombre</w:t></w:r></w:p></w:tc>\
        <w:tc><w:p><w:r><w:t>Edad</w:t></w:r></w:p></w:tc></w:tr>\
        <w:tr>\
        <w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Ana</w:t></w:r></w:p></w:tc>\
        <w:tc><w:p><w:pPr><w:rPr><w:i/></w:rPr></w:pPr></w:p></w:tc></w:tr></w:tbl>";

    fn sample() -> Vec<u8> {
        docx(&format!(
            "<w:document><w:body>\
             <w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Hola </w:t></w:r>\
             <w:r><w:t>mundo</w:t></w:r></w:p>\
             <w:p><w:r><w:t>Precio A &amp; B</w:t></w:r></w:p>\
             {TABLE}</w:body></w:document>"
        ))
    }

    #[test]
    fn replace_text_joins_text_split_across_runs_and_keeps_the_first_format() {
        let out = edit_docx(
            &sample(),
            &[Change::Replace {
                find: "Hola mundo",
                replace: "Adiós",
            }],
        )
        .expect("edit");
        let xml = document_xml(&out);
        assert!(
            xml.contains("<w:b/></w:rPr><w:t xml:space=\"preserve\">Adiós</w:t></w:r>"),
            "{xml}"
        );
        assert!(text_of(&out).starts_with("Adiós\n"), "{}", text_of(&out));
    }

    #[test]
    fn replace_text_matches_escaped_text_and_escapes_the_new_one() {
        let out = edit_docx(
            &sample(),
            &[Change::Replace {
                find: "A & B",
                replace: "C < D",
            }],
        )
        .expect("edit");
        assert!(document_xml(&out).contains("C &lt; D"));
        assert!(text_of(&out).contains("Precio C < D"));
    }

    #[test]
    fn set_cell_keeps_the_format_of_a_filled_cell() {
        let out = edit_docx(
            &sample(),
            &[Change::Cell {
                table: 1,
                row: 2,
                col: 1,
                text: "Luis",
            }],
        )
        .expect("edit");
        let xml = document_xml(&out);
        assert!(
            xml.contains("<w:rPr><w:b/></w:rPr><w:t>Luis</w:t>"),
            "{xml}"
        );
        assert!(!xml.contains("Ana"));
    }

    #[test]
    fn set_cell_creates_a_run_in_an_empty_cell_with_the_mark_format() {
        let out = edit_docx(
            &sample(),
            &[Change::Cell {
                table: 1,
                row: 2,
                col: 2,
                text: "42",
            }],
        )
        .expect("edit");
        assert!(document_xml(&out)
            .contains("<w:r><w:rPr><w:i/></w:rPr><w:t xml:space=\"preserve\">42</w:t></w:r>"));
    }

    #[test]
    fn a_reopened_document_shows_the_new_text_and_the_other_parts_are_unchanged() {
        let before = sample();
        let out = edit_docx(
            &before,
            &[
                Change::Replace {
                    find: "mundo",
                    replace: "gente",
                },
                Change::Cell {
                    table: 1,
                    row: 1,
                    col: 2,
                    text: "Años",
                },
            ],
        )
        .expect("edit");
        assert!(text_of(&out).contains("Hola gente"));
        assert!(text_of(&out).contains("Años"));
        ooxml::check(&out).expect("still a package");

        let read = |bytes: &[u8], name: &[u8]| {
            let entries = ooxml::directory(bytes).expect("directory");
            let entry = entries.iter().find(|e| e.name == name).expect("part");
            ooxml::read_part(bytes, entry).expect("read")
        };
        assert_eq!(
            read(&before, b"word/styles.xml"),
            read(&out, b"word/styles.xml")
        );
        assert_eq!(
            read(&before, b"[Content_Types].xml"),
            read(&out, b"[Content_Types].xml")
        );
    }

    #[test]
    fn an_index_out_of_range_is_a_clear_error() {
        let err = edit_docx(
            &sample(),
            &[Change::Cell {
                table: 2,
                row: 1,
                col: 1,
                text: "x",
            }],
        )
        .expect_err("no table 2");
        assert!(
            matches!(&err, CraftError::Target(msg) if msg.contains("no hay tabla 2")),
            "{err}"
        );
        let err = edit_docx(
            &sample(),
            &[Change::Cell {
                table: 1,
                row: 9,
                col: 1,
                text: "x",
            }],
        )
        .expect_err("no row 9");
        assert!(matches!(err, CraftError::Target(_)));
    }

    #[test]
    fn a_text_that_is_not_there_is_refused() {
        let err = edit_docx(
            &sample(),
            &[Change::Replace {
                find: "no está",
                replace: "x",
            }],
        )
        .expect_err("missing");
        assert!(matches!(err, CraftError::Target(_)));
    }

    #[test]
    fn a_package_without_a_document_is_refused() {
        let bytes = zip(&[("[Content_Types].xml", CONTENT_TYPES, false)]);
        assert!(edit_docx(&bytes, &[]).is_err());
        assert!(edit_docx(b"no es un docx", &[]).is_err());
    }

    #[test]
    fn replacing_a_text_with_spaces_at_its_edges_keeps_them() {
        let out = edit_docx(
            &sample(),
            &[Change::Replace {
                find: "Hola",
                replace: " Buenas",
            }],
        )
        .expect("edit");
        assert!(document_xml(&out).contains("xml:space=\"preserve\"> Buenas </w:t>"));
    }
}
