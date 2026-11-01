//! In-memory reduction of one conversation.
//!
//! The host orders entries by `seq` and may deliver the newest page twice
//! (opening plus a reconnect). This feed is the only place that merges pages,
//! so the screen never duplicates a message and never reorders one.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::chat::{Approval, MailDraftCard, Message, TimelineEntry};

/// One card per delegation: `ref_id` is the delegation id; older hosts may
/// only send `delegation_id`. Card `seq` is stable, so the highest seq wins.
fn card_key(entry: &TimelineEntry) -> Option<&str> {
    entry.ref_id.as_deref().or(entry.delegation_id.as_deref())
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatEntry {
    Message {
        seq: i64,
        message: Message,
    },
    Card {
        seq: i64,
        entry: TimelineEntry,
    },
    /// A line of the timeline for an approval the operator did not decide.
    Approval {
        seq: i64,
        entry: TimelineEntry,
    },
    /// A mail draft the operator opens from the conversation.
    MailDraft {
        seq: i64,
        entry: MailDraftCard,
    },
    Unknown {
        seq: i64,
        entry: TimelineEntry,
    },
}

impl ChatEntry {
    #[cfg(test)]
    fn seq(&self) -> i64 {
        match self {
            ChatEntry::Message { seq, .. }
            | ChatEntry::Card { seq, .. }
            | ChatEntry::Approval { seq, .. }
            | ChatEntry::MailDraft { seq, .. }
            | ChatEntry::Unknown { seq, .. } => *seq,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSnapshot {
    pub conversation_id: String,
    pub entries: Vec<ChatEntry>,
    /// The approvals that wait for the operator in this conversation.
    pub approvals: Vec<Approval>,
    pub next_before_seq: Option<i64>,
    pub last_seq: i64,
    pub last_read_seq: i64,
}

#[derive(Debug, Default)]
pub struct ChatFeed {
    conversation_id: Option<String>,
    entries: BTreeMap<i64, ChatEntry>,
    approvals: Vec<Approval>,
    next_before_seq: Option<i64>,
    last_read_seq: i64,
}

impl ChatFeed {
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    pub fn reset(&mut self, conversation_id: &str) {
        self.conversation_id = Some(conversation_id.to_string());
        self.entries.clear();
        self.approvals.clear();
        self.next_before_seq = None;
        self.last_read_seq = 0;
    }

    /// Reopening the same conversation keeps the page already loaded.
    pub fn ensure(&mut self, conversation_id: &str) {
        if self.conversation_id() != Some(conversation_id) {
            self.reset(conversation_id);
        }
    }

    pub fn merge_messages(&mut self, messages: impl IntoIterator<Item = Message>) {
        for mut message in messages {
            let seq = message.seq;
            // A message page has no suggestions or quote: keep what the timeline gave.
            if let Some(ChatEntry::Message { message: held, .. }) = self.entries.get(&seq) {
                if message.suggestions.is_empty() {
                    message.suggestions = held.suggestions.clone();
                }
                if message.quote.is_none() {
                    message.quote = held.quote.clone();
                }
            }
            self.entries
                .insert(seq, ChatEntry::Message { seq, message });
        }
    }

    /// Message rows only update state and preview: the body comes from the
    /// message sync. Newer kinds are drawn as neutral lines by the shell.
    pub fn merge_timeline(&mut self, entries: impl IntoIterator<Item = TimelineEntry>) {
        for entry in entries {
            let seq = entry.seq;
            match entry.kind.as_str() {
                "message" => {
                    if let Some(ChatEntry::Message { message, .. }) = self.entries.get_mut(&seq) {
                        if let Some(state) = entry.state {
                            message.state = state;
                        }
                        if let Some(turn_state) = entry.turn_state {
                            message.turn_state = Some(turn_state);
                        }
                        message.suggestions = entry.suggestions;
                        message.quote = entry.quote;
                        message.attachments = entry.attachments;
                        if let Some(preview) = entry.preview {
                            if message.body_md.is_empty() {
                                message.body_md = preview;
                            }
                        }
                    }
                }
                "card" => {
                    if let Some(key) = card_key(&entry).map(str::to_string) {
                        let existing = self.entries.iter().find_map(|(seq, entry)| match entry {
                            ChatEntry::Card { entry, .. }
                                if card_key(entry) == Some(key.as_str()) =>
                            {
                                Some(*seq)
                            }
                            _ => None,
                        });
                        match existing {
                            // An older page must not downgrade a newer card.
                            Some(old) if old > seq => continue,
                            Some(old) => {
                                self.entries.remove(&old);
                            }
                            None => {}
                        }
                    }
                    self.entries.insert(seq, ChatEntry::Card { seq, entry });
                }
                "approval" => {
                    self.entries.insert(seq, ChatEntry::Approval { seq, entry });
                }
                "mail_draft" => {
                    if let Some(card) = MailDraftCard::from_entry(entry) {
                        self.entries
                            .insert(seq, ChatEntry::MailDraft { seq, entry: card });
                    }
                }
                _ => {
                    self.entries.insert(seq, ChatEntry::Unknown { seq, entry });
                }
            }
        }
    }

    /// The inbox lists what still waits. An approval that left it was settled
    /// or expired on the host, and the host writes no timeline line for the
    /// operator's own decision, so its card stays until the conversation
    /// changes. A row the inbox still lists is replaced by the fresh one.
    pub fn set_approvals(&mut self, pending: Vec<Approval>) {
        let mut kept: Vec<Approval> = self
            .approvals
            .iter()
            .filter(|held| !pending.iter().any(|row| row.id == held.id))
            .cloned()
            .collect();
        kept.extend(pending);
        self.approvals = kept;
    }

    /// Swaps in the record the host answered for one decision. An approval the
    /// feed does not hold yet is added, so the card never shows a stale state.
    pub fn replace_approval(&mut self, approval: Approval) {
        match self
            .approvals
            .iter_mut()
            .find(|held| held.id == approval.id)
        {
            Some(held) => *held = approval,
            None => self.approvals.push(approval),
        }
    }

    pub fn set_next_before(&mut self, next_before_seq: Option<i64>) {
        self.next_before_seq = next_before_seq;
    }

    pub fn set_last_read(&mut self, last_read_seq: i64) {
        self.last_read_seq = last_read_seq;
    }

    pub fn last_seq(&self) -> i64 {
        self.entries.keys().next_back().copied().unwrap_or(0)
    }

    pub fn snapshot(&self) -> ChatSnapshot {
        ChatSnapshot {
            conversation_id: self.conversation_id.clone().unwrap_or_default(),
            entries: self.entries.values().cloned().collect(),
            approvals: self.approvals.clone(),
            next_before_seq: self.next_before_seq,
            last_seq: self.last_seq(),
            last_read_seq: self.last_read_seq,
        }
    }
}

#[cfg(test)]
mod tests;
