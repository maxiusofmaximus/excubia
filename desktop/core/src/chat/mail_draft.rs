//! The mail draft card of the chat timeline: the draft the operator opens from
//! the conversation. The card never carries the body or the sender.

use serde::Serialize;

use super::TimelineEntry;

#[derive(Debug, Clone, Serialize)]
pub struct MailDraftCard {
    pub account: String,
    pub draft_id: String,
    pub to: Vec<String>,
    pub subject: String,
    pub preview: String,
}

impl MailDraftCard {
    /// A row without its account or draft id has nothing to open: no card.
    pub fn from_entry(entry: TimelineEntry) -> Option<Self> {
        Some(Self {
            account: non_empty(entry.account)?,
            draft_id: non_empty(entry.draft_id)?,
            to: entry.to,
            subject: entry.subject.unwrap_or_default(),
            preview: entry.preview.unwrap_or_default(),
        })
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}
