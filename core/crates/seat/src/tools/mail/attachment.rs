//! `mail.attachment_read` (TASK-ATT1): the content of one file attachment of a received
//! message, read by the host. The parser bounds the fields the way `doc.read` does; the
//! host decides what the file is, and nothing here opens it.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, MAX_ID_CHARS};
use crate::tools::parse::name_field;

/// A read of one attachment of a received message. `sheet` and `range` apply to
/// spreadsheets, `page` to PDF and Word; the host ignores what does not apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentRead {
    pub account: String,
    pub message_id: String,
    pub attachment_id: String,
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub page: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    account: String,
    message_id: String,
    attachment_id: String,
    sheet: Option<String>,
    range: Option<String>,
    page: Option<u32>,
}

pub fn parse(request: &ToolRequest) -> Result<AttachmentRead, ToolError> {
    let args: ReadArgs = arguments(request)?;
    if args.page == Some(0) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "page",
            "page starts at 1",
        ));
    }
    Ok(AttachmentRead {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
        message_id: bounded("message_id", &args.message_id, MAX_ID_CHARS)?,
        attachment_id: bounded("attachment_id", &args.attachment_id, MAX_ID_CHARS)?,
        sheet: args
            .sheet
            .map(|sheet| name_field("sheet", &sheet))
            .transpose()?,
        range: args
            .range
            .map(|range| name_field("range", &range))
            .transpose()?,
        page: args.page,
    })
}
