//! The document tools (DOC5B, DOC8): `doc.read` reads a document the user sent in the
//! chat, `doc.create` makes a new one. The parser bounds the fields; the host finds the
//! files and writes them, and nothing here touches either.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::parse::{arguments, name_field};

/// A read of one chat attachment. `sheet` and `range` apply to spreadsheets, `page`
/// to PDF and Word; the host ignores what does not apply to the document it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocRead {
    pub attachment_id: String,
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub page: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    attachment_id: String,
    sheet: Option<String>,
    range: Option<String>,
    page: Option<u32>,
}

pub fn read(request: &ToolRequest) -> Result<DocRead, ToolError> {
    let args: ReadArgs = arguments(request, "attachment_id")?;
    let page = match args.page {
        Some(0) => {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "page",
                "page starts at 1",
            ))
        }
        other => other,
    };
    Ok(DocRead {
        attachment_id: name_field("attachment_id", &args.attachment_id)?,
        sheet: args
            .sheet
            .map(|sheet| name_field("sheet", &sheet))
            .transpose()?,
        range: args
            .range
            .map(|range| name_field("range", &range))
            .transpose()?,
        page,
    })
}

/// The most text a new document may be made from: the size of a chat file.
pub const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;

/// The formats `doc.create` makes. Anything else is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocFormat {
    Xlsx,
    Docx,
    Pdf,
    Csv,
    Txt,
    Md,
    Json,
}

impl DocFormat {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "xlsx" => Self::Xlsx,
            "docx" => Self::Docx,
            "pdf" => Self::Pdf,
            "csv" => Self::Csv,
            "txt" => Self::Txt,
            "md" => Self::Md,
            "json" => Self::Json,
            _ => return None,
        })
    }
}

/// A new document: its format, the file name Alfred gave (the host makes it safe) and
/// its text, kept as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocCreate {
    pub format: DocFormat,
    pub name: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    format: String,
    name: String,
    content: String,
}

pub fn create(request: &ToolRequest) -> Result<DocCreate, ToolError> {
    let args: CreateArgs = arguments(request, "format")?;
    let format = DocFormat::parse(&args.format).ok_or_else(|| {
        ToolError::invalid(
            "invalid_arguments",
            "format",
            "format must be xlsx, docx, pdf, csv, txt, md or json",
        )
    })?;
    if args.content.trim().is_empty() {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "content",
            "content cannot be empty",
        ));
    }
    if args.content.len() > MAX_CONTENT_BYTES {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "content",
            "content is at most 10 MiB",
        ));
    }
    Ok(DocCreate {
        format,
        name: name_field("name", &args.name)?,
        content: args.content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "doc.read".to_string(),
            arguments,
        }
    }

    #[test]
    fn only_the_attachment_is_required() {
        let read = read(&request(
            json!({ "attachment_id": "att_01J5Z8K2M3N4P5Q6R7S8T9V0WX" }),
        ))
        .expect("read");
        assert_eq!(
            read,
            DocRead {
                attachment_id: "att_01J5Z8K2M3N4P5Q6R7S8T9V0WX".to_string(),
                sheet: None,
                range: None,
                page: None,
            }
        );
    }

    #[test]
    fn sheet_range_and_page_travel_as_given() {
        let read = read(&request(json!({
            "attachment_id": "att_1",
            "sheet": "Resumen",
            "range": "A1:H40",
            "page": 2
        })))
        .expect("read");
        assert_eq!(read.sheet.as_deref(), Some("Resumen"));
        assert_eq!(read.range.as_deref(), Some("A1:H40"));
        assert_eq!(read.page, Some(2));
    }

    #[test]
    fn page_zero_and_negative_pages_are_refused() {
        let zero =
            read(&request(json!({ "attachment_id": "att_1", "page": 0 }))).expect_err("page 0");
        assert_eq!(zero.code, "invalid_arguments");
        assert_eq!(zero.field, Some("page"));
        assert!(read(&request(json!({ "attachment_id": "att_1", "page": -1 }))).is_err());
        assert!(read(&request(json!({ "attachment_id": "att_1", "page": "2" }))).is_err());
    }

    fn create_request(arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: "doc.create".to_string(),
            arguments,
        }
    }

    #[test]
    fn every_allowed_format_is_typed_and_the_rest_is_refused() {
        for (text, format) in [
            ("xlsx", DocFormat::Xlsx),
            ("docx", DocFormat::Docx),
            ("pdf", DocFormat::Pdf),
            ("csv", DocFormat::Csv),
            ("txt", DocFormat::Txt),
            ("md", DocFormat::Md),
            ("json", DocFormat::Json),
        ] {
            let created = create(&create_request(
                json!({ "format": text, "name": "Datos", "content": "a,b" }),
            ))
            .expect(text);
            assert_eq!(created.format, format, "{text}");
            assert_eq!(created.name, "Datos");
            assert_eq!(created.content, "a,b");
        }
        for refused in ["xls", "xlsm", "docm", "doc", "exe", ""] {
            let error = create(&create_request(
                json!({ "format": refused, "name": "x", "content": "x" }),
            ))
            .expect_err(refused);
            assert_eq!(error.code, "invalid_arguments", "{refused}");
            assert_eq!(error.field, Some("format"), "{refused}");
        }
    }

    #[test]
    fn name_and_content_are_required_and_content_is_bounded() {
        assert!(create(&create_request(json!({ "format": "txt", "content": "x" }))).is_err());
        assert!(create(&create_request(
            json!({ "format": "txt", "name": "  ", "content": "x" })
        ))
        .is_err());
        assert!(create(&create_request(
            json!({ "format": "txt", "name": "x", "content": "" })
        ))
        .is_err());
        let too_big = "a".repeat(MAX_CONTENT_BYTES + 1);
        let error = create(&create_request(
            json!({ "format": "csv", "name": "x", "content": too_big }),
        ))
        .expect_err("content past the cap");
        assert_eq!(error.field, Some("content"));
    }

    #[test]
    fn a_name_with_separators_travels_to_the_host_which_makes_it_safe() {
        let created = create(&create_request(
            json!({ "format": "txt", "name": "../x\\y.txt", "content": "x" }),
        ))
        .expect("parsed");
        assert_eq!(created.name, "../x\\y.txt");
    }

    #[test]
    fn an_unknown_field_is_refused_for_create_too() {
        assert!(create(&create_request(json!({
            "format": "txt", "name": "x", "content": "x", "path": "C:\\x.txt"
        })))
        .is_err());
    }

    #[test]
    fn a_missing_or_empty_attachment_is_refused() {
        assert!(read(&request(json!({}))).is_err());
        assert!(read(&request(json!({ "attachment_id": "  " }))).is_err());
    }

    #[test]
    fn an_unknown_field_is_refused_never_ignored() {
        assert!(read(&request(
            json!({ "attachment_id": "att_1", "path": "C:\\x.xlsx" })
        ))
        .is_err());
    }

    #[test]
    fn sheet_and_range_are_bounded() {
        let long = "x".repeat(201);
        assert!(read(&request(json!({ "attachment_id": "att_1", "sheet": long }))).is_err());
        assert!(read(&request(
            json!({ "attachment_id": "att_1", "range": "A\u{0}1" })
        ))
        .is_err());
    }
}
