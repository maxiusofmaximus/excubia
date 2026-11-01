//! `mail.triage` (F0): the host lists a folder, reads each message and gives it the
//! verdict of the fixed rules over its checks. A read that fails keeps its entry as
//! `unknown` with the error's code, and the rest go on. No body and no link leaves
//! the host, and each message read is audited by its id, never by its subject.

use hn_core_mail::inbox::{InboxItem, ReceivedMessage};
use hn_core_mail::received::Checks;
use hn_core_mail::triage::{verdict, Level, Signal};
use hn_core_mail::{MailError, ReadRecord};
use hn_core_seat::TriageQuery;
use serde::Serialize;
use serde_json::{json, Value};

use crate::mail_read::NOTICE;
use crate::mail_shadow::Batch;
use crate::mail_tools::MailTools;

impl MailTools {
    /// One listing, then one read per message, in order. Only a read that succeeded
    /// is audited.
    pub(crate) async fn triage(&self, query: &TriageQuery) -> Result<Value, MailError> {
        let access = self.session(&query.account).await?;
        let client = self.client(&access, &query.account);
        let items = client
            .list_inbox(
                query.folder.graph_name(),
                query.limit,
                query.unread_only,
                None,
            )
            .await?;
        let mut reads = Vec::with_capacity(items.len());
        for item in &items {
            let read = client.read_message(&item.id).await;
            if read.is_ok() {
                self.audit.append(&ReadRecord::new(
                    &query.account,
                    "mail.triage",
                    Some(&item.id),
                ))?;
            }
            reads.push(read);
        }
        let messages: Vec<Entry> = items
            .iter()
            .zip(&reads)
            .map(|(item, read)| Entry::of(item, read.as_ref()))
            .collect();
        let reply = json!({
            "ok": true,
            "untrusted_content": true,
            "notice": NOTICE,
            "account": query.account,
            "folder": query.folder.graph_name(),
            "messages": messages,
        });
        if let Some(shadow) = &self.shadow {
            shadow.start(Batch {
                account: query.account.clone(),
                folder: query.folder.graph_name(),
                messages: reads.into_iter().filter_map(Result::ok).collect(),
            });
        }
        Ok(reply)
    }
}

/// One message of the reply. A failed read has no `checks`, and its `error` is the code.
#[derive(Serialize)]
struct Entry<'a> {
    id: &'a str,
    from_name: &'a str,
    from_address: &'a str,
    subject: &'a str,
    received_at: &'a str,
    level: Level,
    signals: Vec<Signal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checks: Option<&'a Checks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

impl<'a> Entry<'a> {
    fn of(item: &'a InboxItem, read: Result<&'a ReceivedMessage, &MailError>) -> Self {
        let mut entry = Entry {
            id: &item.id,
            from_name: &item.from_name,
            from_address: &item.from_address,
            subject: &item.subject,
            received_at: &item.received_at,
            level: Level::Unknown,
            signals: Vec::new(),
            checks: None,
            error: None,
        };
        match read {
            Ok(message) => {
                let found = verdict(&message.checks);
                entry.level = found.level;
                entry.signals = found.signals;
                entry.checks = Some(&message.checks);
            }
            Err(error) => entry.error = Some(error.code()),
        }
        entry
    }
}
