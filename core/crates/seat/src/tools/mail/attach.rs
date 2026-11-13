//! `mail.attach_file` (MAIL1D, DOC2): a file goes to a draft from one of two places.
//! `attachment_id` names a file the user sent in this conversation, which needs no
//! approval; `path` names a file on this PC, which the policy may send to the chat for
//! the operator's answer. Exactly one of them is given.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, invalid, MailCall, MAX_ID_CHARS, MAX_PATH_CHARS};

/// Where the file comes from. The host resolves a chat file itself; a path is only ever
/// what the operator named, checked by the path policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachSource {
    /// The id of a file the user sent in this conversation (`att_…`).
    Chat(String),
    /// A local path, as the operator named it.
    Path(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailAttach {
    pub account: String,
    pub draft_id: String,
    pub source: AttachSource,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachArgs {
    account: String,
    draft_id: String,
    attachment_id: Option<String>,
    path: Option<String>,
}

pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let args: AttachArgs = arguments(request)?;
    let source = match (args.attachment_id, args.path) {
        (Some(id), None) => AttachSource::Chat(bounded("attachment_id", &id, MAX_ID_CHARS)?),
        (None, Some(path)) => AttachSource::Path(bounded("path", &path, MAX_PATH_CHARS)?),
        _ => {
            return Err(invalid(
                "attachment_id",
                "give attachment_id (a file the user sent in this chat) or path (a file on this PC), not both and not neither".into(),
            ))
        }
    };
    Ok(MailCall::AttachFile(MailAttach {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
        draft_id: bounded("draft_id", &args.draft_id, MAX_ID_CHARS)?,
        source,
    }))
}
