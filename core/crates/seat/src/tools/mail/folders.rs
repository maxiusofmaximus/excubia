//! `mail.folders` (F2A): the top-level folders of one account. It takes the account
//! and nothing else; it is a read, and the folder ids it returns are never destinations.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, MailCall, MAX_ID_CHARS};

/// The listing of one account's folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldersQuery {
    pub account: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FoldersArgs {
    account: String,
}

pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let args: FoldersArgs = arguments(request)?;
    Ok(MailCall::Folders(FoldersQuery {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
    }))
}
