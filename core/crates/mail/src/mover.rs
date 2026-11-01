//! Moving one message to the junk folder, to the deleted items or back to the inbox
//! (F2A). Graph copies the message and deletes the original, so the id changes: a
//! [`Moved`] carries the new id and the folder the message left. Nothing here
//! reaches a folder by its opaque id, and nothing is permanently deleted.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::MailError;
use crate::folders::FolderAliases;
use crate::graph::{check_status, encode_segment, GraphClient, GRAPH_BASE};
use crate::inbox::parse;
use crate::ledger::digest_hex;

/// The folder a message is reported to have left when it is not a well-known one.
pub const OTHER_FOLDER: &str = "other";

/// Where a move may send a message. Closed on purpose: the three Graph aliases only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDest {
    Junk,
    Deleted,
    Inbox,
}

impl MoveDest {
    /// The Graph well-known name this destination is sent as.
    pub fn alias(self) -> &'static str {
        match self {
            Self::Junk => "junkemail",
            Self::Deleted => "deleteditems",
            Self::Inbox => "inbox",
        }
    }

    /// The destination a stored folder name stands for; `None` for any other name.
    pub fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "junkemail" => Some(Self::Junk),
            "deleteditems" => Some(Self::Deleted),
            "inbox" => Some(Self::Inbox),
            _ => None,
        }
    }
}

/// The result of a move: the message's new id and the folder it left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub id: String,
    pub from_folder: String,
}

/// The digest a batch of moves binds: the account and each message with its
/// destination, ordered by message id, so one batch is one question.
pub fn batch_digest<'a>(
    account: &str,
    moves: impl IntoIterator<Item = (&'a str, MoveDest)>,
) -> String {
    let mut sorted: Vec<(&str, &str)> = moves
        .into_iter()
        .map(|(id, dest)| (id, dest.alias()))
        .collect();
    sorted.sort_unstable();
    let items: Vec<Value> = sorted
        .iter()
        .map(|(id, dest)| json!({ "message_id": id, "dest": dest }))
        .collect();
    let canonical = json!({ "account": account, "items": items });
    format!("sha256:{}", digest_hex(canonical.to_string().as_bytes()))
}

impl GraphClient<'_> {
    /// Moves one message. Eight requests: the message's parent folder, the six
    /// aliases by path and the move itself.
    pub async fn move_message(&self, message_id: &str, dest: MoveDest) -> Result<Moved, MailError> {
        let parent = self.parent_folder_id(message_id).await?;
        let aliases = self.folder_aliases().await?;
        self.move_from(message_id, dest, &parent, &aliases).await
    }

    /// Moves one message with the aliases already resolved: a batch resolves them
    /// once and then makes two requests per message, the parent and the move.
    pub async fn move_message_with(
        &self,
        message_id: &str,
        dest: MoveDest,
        aliases: &FolderAliases,
    ) -> Result<Moved, MailError> {
        let parent = self.parent_folder_id(message_id).await?;
        self.move_from(message_id, dest, &parent, aliases).await
    }

    /// The move itself, once the parent folder and the aliases are known.
    async fn move_from(
        &self,
        message_id: &str,
        dest: MoveDest,
        parent: &str,
        aliases: &FolderAliases,
    ) -> Result<Moved, MailError> {
        let from_folder = aliases.alias_of(parent).unwrap_or(OTHER_FOLDER).to_owned();
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/move",
            encode_segment(message_id)
        );
        let body = json!({ "destinationId": dest.alias() });
        let response = self
            .transport
            .post_json(&url, self.access_token, &body)
            .await?;
        if response.status == 404 {
            return Err(MailError::MessageNotFound);
        }
        check_status(&response)?;
        let raw: RawMoved = parse(&response, "the moved message is incomplete")?;
        Ok(Moved {
            id: raw.id,
            from_folder,
        })
    }

    /// The id of the folder a message is in; it is compared with the aliases by the caller.
    async fn parent_folder_id(&self, message_id: &str) -> Result<String, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}?$select=parentFolderId",
            encode_segment(message_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        if response.status == 404 {
            return Err(MailError::MessageNotFound);
        }
        check_status(&response)?;
        let parent: RawParent = parse(&response, "the message is incomplete")?;
        Ok(parent.parent_folder_id)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawParent {
    parent_folder_id: String,
}

#[derive(Deserialize)]
struct RawMoved {
    id: String,
}
