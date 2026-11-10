//! The `mail.*` MCP tools (MAIL1D §3, MAIL-LECTURA). Twelve exist: list the
//! accounts, create, update and read a draft, attach a local file to a draft, and
//! read the inbox: list a folder, read one message, and triage a folder (F0), and
//! list the folders of an account (F2A). The reads change nothing. The organize
//! tools (F2b2) propose a batch of moves for the operator to decide, apply the
//! decided batch, and undo one move.
//! There is no send and no delete, and an argument the schema does not name is
//! refused, so nothing here can send a message.

use excubia_core_chat::{ToolError, ToolRequest};
use serde::de::DeserializeOwned;
use serde::Deserialize;

mod attach;
mod folder;
mod folders;
mod organize;
mod triage;

pub use attach::{AttachSource, MailAttach};
pub use folder::InboxFolder;
pub use folders::FoldersQuery;
pub use organize::{
    MoveApply, MoveFolder, MoveProposal, MoveUndo, Organize, ProposedMove, UndoList,
};
pub use triage::TriageQuery;

const MAX_ID_CHARS: usize = 512;
const MAX_ADDRESS_CHARS: usize = 320;
const MAX_RECIPIENTS: usize = 50;
const MAX_SUBJECT_CHARS: usize = 998;
const MAX_BODY_CHARS: usize = 32_768;
const MAX_PATH_CHARS: usize = 1024;
const DEFAULT_LIMIT: i64 = 10;
const MAX_LIMIT: i64 = 20;

/// A typed `mail.*` call. The host turns it into Graph requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailCall {
    Accounts,
    DraftCreate(DraftFields),
    DraftUpdate(DraftUpdate),
    DraftGet(DraftRef),
    AttachFile(MailAttach),
    InboxList(InboxQuery),
    MessageGet(MessageRef),
    Triage(TriageQuery),
    Folders(FoldersQuery),
    Organize(Organize),
}

/// A read of one folder. `limit` is 1..=20; `from` is an address or a domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxQuery {
    pub account: String,
    pub folder: InboxFolder,
    pub limit: u32,
    pub unread_only: bool,
    pub from: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRef {
    pub account: String,
    pub message_id: String,
}

/// The content of a draft, checked. `format` says how `body` is to be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftFields {
    pub account: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub format: BodyFormat,
}

/// `Agent`: the body is plain text, laid out by the core's fixed template with
/// the subject as its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyFormat {
    Text,
    Html,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftUpdate {
    pub draft_id: String,
    pub fields: DraftFields,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftRef {
    pub account: String,
    pub draft_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

/// The fields of a draft as the model sends them. Unknown fields are refused.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftArgs {
    account: String,
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    subject: String,
    body: String,
    #[serde(default)]
    body_format: Option<String>,
}

/// An update names the draft and carries the same fields as a create. It is a
/// separate struct on purpose: `flatten` would let unknown fields through.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftUpdateArgs {
    draft_id: String,
    account: String,
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    subject: String,
    body: String,
    #[serde(default)]
    body_format: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRefArgs {
    account: String,
    draft_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InboxArgs {
    account: String,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    unread_only: bool,
    #[serde(default)]
    from: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageArgs {
    account: String,
    message_id: String,
}

/// Parses one of the `mail.*` tools. The caller routes only those names here.
pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    match request.tool.as_str() {
        "mail.accounts" => {
            let _: NoArgs = arguments(request)?;
            Ok(MailCall::Accounts)
        }
        "mail.draft_create" => {
            let args: DraftArgs = arguments(request)?;
            Ok(MailCall::DraftCreate(draft_fields(&Draft {
                account: &args.account,
                to: &args.to,
                cc: &args.cc,
                subject: &args.subject,
                body: &args.body,
                body_format: args.body_format.as_deref(),
            })?))
        }
        "mail.draft_update" => {
            let args: DraftUpdateArgs = arguments(request)?;
            Ok(MailCall::DraftUpdate(DraftUpdate {
                draft_id: bounded("draft_id", &args.draft_id, MAX_ID_CHARS)?,
                fields: draft_fields(&Draft {
                    account: &args.account,
                    to: &args.to,
                    cc: &args.cc,
                    subject: &args.subject,
                    body: &args.body,
                    body_format: args.body_format.as_deref(),
                })?,
            }))
        }
        "mail.draft_get" => {
            let args: DraftRefArgs = arguments(request)?;
            Ok(MailCall::DraftGet(DraftRef {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                draft_id: bounded("draft_id", &args.draft_id, MAX_ID_CHARS)?,
            }))
        }
        "mail.attach_file" => attach::parse(request),
        "mail.inbox_list" => {
            let args: InboxArgs = arguments(request)?;
            Ok(MailCall::InboxList(InboxQuery {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                folder: inbox_folder(args.folder.as_deref())?,
                limit: limit_up_to(args.limit, MAX_LIMIT)?,
                unread_only: args.unread_only,
                from: args
                    .from
                    .as_deref()
                    .map(|from| bounded("from", from, MAX_ADDRESS_CHARS))
                    .transpose()?,
            }))
        }
        "mail.triage" => triage::parse(request),
        "mail.folders" => folders::parse(request),
        "mail.move_propose" | "mail.move_apply" | "mail.undo" | "mail.undo_list" => {
            organize::parse(request)
        }
        "mail.message_get" => {
            let args: MessageArgs = arguments(request)?;
            Ok(MailCall::MessageGet(MessageRef {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                message_id: bounded("message_id", &args.message_id, MAX_ID_CHARS)?,
            }))
        }
        other => Err(invalid("tool", format!("{other:?} is not a mail tool"))),
    }
}

fn arguments<T: DeserializeOwned>(request: &ToolRequest) -> Result<T, ToolError> {
    serde_json::from_value(request.arguments.clone()).map_err(|error| {
        invalid(
            "arguments",
            format!("{} takes only its named fields: {error}", request.tool),
        )
    })
}

fn invalid(field: &'static str, message: String) -> ToolError {
    ToolError::invalid("mail_invalid_input", field, message)
}

/// One line of text with no control characters, between 1 and `max` characters.
fn bounded(field: &'static str, raw: &str, max: usize) -> Result<String, ToolError> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(invalid(
            field,
            format!("{field} must be 1..={max} characters without controls"),
        ));
    }
    Ok(value.to_string())
}

fn subject(raw: &str) -> Result<String, ToolError> {
    if raw.chars().count() > MAX_SUBJECT_CHARS || raw.chars().any(char::is_control) {
        return Err(invalid(
            "subject",
            format!("subject must be at most {MAX_SUBJECT_CHARS} characters without controls"),
        ));
    }
    Ok(raw.to_string())
}

/// Line breaks and tabs are part of a body; other controls are not.
fn body(raw: &str) -> Result<String, ToolError> {
    let controlled = raw
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'));
    if raw.chars().count() > MAX_BODY_CHARS || controlled {
        return Err(invalid(
            "body",
            format!("body must be at most {MAX_BODY_CHARS} characters without controls"),
        ));
    }
    Ok(raw.to_string())
}

fn recipients(
    field: &'static str,
    raw: &[String],
    required: bool,
) -> Result<Vec<String>, ToolError> {
    if required && raw.is_empty() {
        return Err(invalid(
            field,
            format!("{field} needs at least one address"),
        ));
    }
    if raw.len() > MAX_RECIPIENTS {
        return Err(invalid(
            field,
            format!("{field} takes at most {MAX_RECIPIENTS} addresses"),
        ));
    }
    raw.iter()
        .map(|address| bounded(field, address, MAX_ADDRESS_CHARS))
        .collect()
}

/// The draft fields of either argument struct, borrowed so both share one check.
struct Draft<'a> {
    account: &'a str,
    to: &'a [String],
    cc: &'a [String],
    subject: &'a str,
    body: &'a str,
    body_format: Option<&'a str>,
}

/// The fields of a draft checked with the tools' own limits, for the owner's edit
/// of a draft (MAILUX-HOST §B). `body_format` is `text` or `html`, as Graph stores it.
pub fn check_draft_fields(
    account: &str,
    to: &[String],
    cc: &[String],
    subject: &str,
    body: &str,
    body_format: &str,
) -> Result<DraftFields, ToolError> {
    draft_fields(&Draft {
        account,
        to,
        cc,
        subject,
        body,
        body_format: Some(body_format),
    })
}

fn draft_fields(draft: &Draft) -> Result<DraftFields, ToolError> {
    Ok(DraftFields {
        account: bounded("account", draft.account, MAX_ID_CHARS)?,
        to: recipients("to", draft.to, true)?,
        cc: recipients("cc", draft.cc, false)?,
        subject: subject(draft.subject)?,
        body: body(draft.body)?,
        format: body_format(draft.body_format)?,
    })
}

/// An omitted folder is the inbox. The three names are the only ones Graph's
/// well-known folders this surface offers.
fn inbox_folder(raw: Option<&str>) -> Result<InboxFolder, ToolError> {
    match raw {
        None | Some("inbox") => Ok(InboxFolder::Inbox),
        Some("sentitems") => Ok(InboxFolder::SentItems),
        Some("junkemail") => Ok(InboxFolder::JunkEmail),
        Some(_) => Err(invalid(
            "folder",
            "folder must be inbox, sentitems or junkemail".into(),
        )),
    }
}

/// A limit is refused when it is outside 1..=max; it is never clamped in silence.
fn limit_up_to(raw: Option<i64>, max: i64) -> Result<u32, ToolError> {
    match raw.unwrap_or(DEFAULT_LIMIT) {
        limit if (1..=max).contains(&limit) => Ok(limit as u32),
        _ => Err(invalid("limit", format!("limit must be 1..={max}"))),
    }
}

fn body_format(raw: Option<&str>) -> Result<BodyFormat, ToolError> {
    match raw {
        None | Some("text") => Ok(BodyFormat::Text),
        Some("html") => Ok(BodyFormat::Html),
        Some("agent") => Ok(BodyFormat::Agent),
        Some(_) => Err(invalid(
            "body_format",
            "body_format must be text, html or agent".into(),
        )),
    }
}

#[cfg(test)]
mod tests;
