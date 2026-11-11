//! Listing the mail folders (F2A): the top-level folders of the mailbox, read only.
//! A folder id is opaque. It is compared with a message's parent folder and never
//! offered as a destination: moves name a well-known alias ([`crate::mover`]).
//!
//! Graph has no `wellKnownName` property on a folder, but it answers each alias by
//! its path. The aliases are resolved that way and their ids are matched.

use serde::{Deserialize, Serialize};

use crate::error::MailError;
use crate::graph::{check_status, GraphClient, GRAPH_BASE};
use crate::inbox::parse;
use crate::received::one_line;

const FOLDER_SELECT: &str = "id,displayName,totalItemCount,unreadItemCount";
const MAX_FOLDER_NAME_CHARS: usize = 320;
/// The only well-known names kept. Any other folder is listed without one.
const WELL_KNOWN: [&str; 6] = [
    "inbox",
    "junkemail",
    "deleteditems",
    "sentitems",
    "drafts",
    "archive",
];

/// One top-level folder. `well_known_name` is set only for the names in `WELL_KNOWN`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub id: String,
    pub display_name: String,
    pub well_known_name: Option<String>,
    pub total_item_count: u64,
    pub unread_item_count: u64,
}

/// A well-known alias and the id of the folder Graph answers for it.
struct AliasFolder {
    alias: &'static str,
    id: String,
}

/// The well-known folders of one mailbox, resolved once. A batch of moves reuses
/// them, so the six lookups are not repeated for each message.
pub struct FolderAliases(Vec<AliasFolder>);

impl FolderAliases {
    /// The alias a folder id stands for; `None` for a folder that is not one of them.
    pub(crate) fn alias_of(&self, folder_id: &str) -> Option<&'static str> {
        self.0
            .iter()
            .find(|found| found.id == folder_id)
            .map(|found| found.alias)
    }
}

impl GraphClient<'_> {
    /// Lists up to 50 top-level folders and names the ones that are aliases.
    /// Read only: one GET for the list and one per alias.
    pub async fn list_folders(&self) -> Result<Vec<Folder>, MailError> {
        let url = format!("{GRAPH_BASE}/me/mailFolders?$top=50&$select={FOLDER_SELECT}");
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawFolders = parse(&response, "the folder list is incomplete")?;
        let aliases = self.folder_aliases().await?;
        Ok(raw
            .value
            .into_iter()
            .map(|folder| folder.into_folder(&aliases))
            .collect())
    }

    /// The aliases this mailbox has, each with its folder id. An alias the mailbox
    /// does not have (404) is skipped; any other failure stops the lookup.
    pub async fn folder_aliases(&self) -> Result<FolderAliases, MailError> {
        let mut found = Vec::new();
        for alias in WELL_KNOWN {
            let url = format!("{GRAPH_BASE}/me/mailFolders/{alias}?$select=id");
            let response = self.transport.get_json(&url, self.access_token).await?;
            if response.status == 404 {
                continue;
            }
            check_status(&response)?;
            let raw: RawAliasFolder = parse(&response, "the folder alias is incomplete")?;
            found.push(AliasFolder { alias, id: raw.id });
        }
        Ok(FolderAliases(found))
    }
}

#[derive(Deserialize)]
struct RawFolders {
    #[serde(default)]
    value: Vec<RawFolder>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFolder {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    total_item_count: u64,
    #[serde(default)]
    unread_item_count: u64,
}

impl RawFolder {
    fn into_folder(self, aliases: &FolderAliases) -> Folder {
        let well_known_name = aliases.alias_of(&self.id).map(str::to_owned);
        Folder {
            well_known_name,
            id: self.id,
            display_name: one_line(&self.display_name, MAX_FOLDER_NAME_CHARS),
            total_item_count: self.total_item_count,
            unread_item_count: self.unread_item_count,
        }
    }
}

#[derive(Deserialize)]
struct RawAliasFolder {
    id: String,
}
