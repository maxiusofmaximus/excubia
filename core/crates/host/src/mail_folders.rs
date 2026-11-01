//! `mail.folders` (F2A): the top-level folders of one account, read only. One GET for
//! the list; the read is audited by account and tool, never by a folder name.

use hn_core_mail::{MailError, ReadRecord};
use hn_core_seat::FoldersQuery;
use serde_json::Value;

use crate::mail_read;
use crate::mail_tools::MailTools;

impl MailTools {
    pub(crate) async fn folders(&self, query: &FoldersQuery) -> Result<Value, MailError> {
        let access = self.session(&query.account).await?;
        let client = self.client(&access, &query.account);
        let folders = client.list_folders().await?;
        self.audit
            .append(&ReadRecord::new(&query.account, "mail.folders", None))?;
        Ok(mail_read::folders_reply(&folders))
    }
}
