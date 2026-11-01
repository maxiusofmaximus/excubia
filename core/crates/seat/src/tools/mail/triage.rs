//! `mail.triage` (F0): one folder, the inbox or the junk folder, and at most ten
//! messages, since each one costs a read. It takes the inbox arguments but no `from`.

use hn_core_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, invalid, limit_up_to, InboxFolder, MailCall, MAX_ID_CHARS};

const MAX_TRIAGE_LIMIT: i64 = 10;

/// A triage of one folder. `folder` is the inbox or the junk folder only, and
/// `limit` is 1..=10: each message costs one read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriageQuery {
    pub account: String,
    pub folder: InboxFolder,
    pub limit: u32,
    pub unread_only: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TriageArgs {
    account: String,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    unread_only: bool,
}

pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let args: TriageArgs = arguments(request)?;
    Ok(MailCall::Triage(TriageQuery {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
        folder: triage_folder(args.folder.as_deref())?,
        limit: limit_up_to(args.limit, MAX_TRIAGE_LIMIT)?,
        unread_only: args.unread_only,
    }))
}

/// The sent items are not triaged: only the folders where mail arrives.
fn triage_folder(raw: Option<&str>) -> Result<InboxFolder, ToolError> {
    match raw {
        None | Some("inbox") => Ok(InboxFolder::Inbox),
        Some("junkemail") => Ok(InboxFolder::JunkEmail),
        Some(_) => Err(invalid(
            "folder",
            "folder must be inbox or junkemail".into(),
        )),
    }
}
