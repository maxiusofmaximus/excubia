//! In-memory reduction of one conversation.
//!
//! The host orders entries by `seq` and may deliver the newest page twice
//! (opening plus a reconnect). This feed is the only place that merges pages,
//! so the screen never duplicates a message and never reorders one.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::chat::{Approval, Message, TimelineEntry};

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
        for message in messages {
            let seq = message.seq;
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
mod tests {
    use super::*;

    fn message(seq: i64, author: &str, body: &str) -> Message {
        Message {
            id: format!("msg_{seq}"),
            conversation_id: "cnv_1".to_string(),
            seq,
            author_kind: author.to_string(),
            author_agent_id: None,
            body_md: body.to_string(),
            state: "complete".to_string(),
            created_at: "2026-10-22T10:00:00.000Z".to_string(),
        }
    }

    fn card(seq: i64, delegation_id: &str, status: &str) -> TimelineEntry {
        let mut entry = timeline(seq, "card", None);
        entry.ref_id = Some(delegation_id.to_string());
        entry.delegation_id = Some(delegation_id.to_string());
        entry.status = Some(status.to_string());
        entry.title = Some(format!("delegación {delegation_id}"));
        entry
    }

    fn timeline(seq: i64, kind: &str, state: Option<&str>) -> TimelineEntry {
        TimelineEntry {
            seq,
            kind: kind.to_string(),
            message_id: Some(format!("msg_{seq}")),
            author_kind: None,
            author_agent_id: None,
            preview: Some("vista previa".to_string()),
            state: state.map(str::to_string),
            card_id: None,
            delegation_id: None,
            ref_id: None,
            title: None,
            summary: None,
            status: None,
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            created_at: "2026-10-22T10:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn pages_are_ordered_by_seq_even_when_they_arrive_out_of_order() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_messages([message(4, "agent", "cuatro"), message(2, "user", "dos")]);
        feed.merge_messages([message(3, "user", "tres"), message(1, "agent", "uno")]);
        let seqs: Vec<i64> = feed.snapshot().entries.iter().map(ChatEntry::seq).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_reconnect_that_replays_the_same_page_does_not_duplicate() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        let page = vec![message(1, "user", "hola"), message(2, "agent", "hecho")];
        feed.merge_messages(page.clone());
        feed.merge_messages(page);
        assert_eq!(feed.snapshot().entries.len(), 2);
        assert_eq!(feed.last_seq(), 2);

        feed.merge_messages([message(3, "user", "otra")]);
        assert_eq!(feed.snapshot().entries.len(), 3);
    }

    #[test]
    fn a_timeline_update_changes_the_state_without_losing_the_body() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_messages([message(1, "agent", "respuesta real")]);
        feed.merge_timeline([timeline(1, "message", Some("failed"))]);
        match &feed.snapshot().entries[0] {
            ChatEntry::Message { message, .. } => {
                assert_eq!(message.state, "failed");
                assert_eq!(message.body_md, "respuesta real");
            }
            other => panic!("esperaba un mensaje, llegó {other:?}"),
        }
    }

    #[test]
    fn cards_and_unknown_kinds_survive_a_newer_host() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_timeline([timeline(2, "card", None), timeline(3, "mystery", None)]);
        let snapshot = feed.snapshot();
        assert!(matches!(snapshot.entries[0], ChatEntry::Card { .. }));
        assert!(matches!(snapshot.entries[1], ChatEntry::Unknown { .. }));
    }

    #[test]
    fn an_approval_line_is_its_own_entry_and_pending_approvals_ride_along() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_timeline([timeline(4, "approval", None)]);
        feed.set_approvals(vec![Approval {
            id: "apr_01J1".to_string(),
            level: Some("user".to_string()),
            ..Approval::default()
        }]);
        let snapshot = feed.snapshot();
        assert!(matches!(
            snapshot.entries[0],
            ChatEntry::Approval { seq: 4, .. }
        ));
        assert_eq!(snapshot.approvals.len(), 1);
        assert_eq!(snapshot.approvals[0].id, "apr_01J1");

        feed.set_approvals(Vec::new());
        assert_eq!(
            feed.snapshot().approvals.len(),
            1,
            "an approval that left the inbox keeps its card"
        );
    }

    #[test]
    fn a_fresh_inbox_row_replaces_the_held_one() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.set_approvals(vec![Approval {
            id: "apr_01J3".to_string(),
            status: "pending".to_string(),
            ..Approval::default()
        }]);
        feed.set_approvals(vec![Approval {
            id: "apr_01J3".to_string(),
            status: "expired".to_string(),
            ..Approval::default()
        }]);
        let approvals = feed.snapshot().approvals;
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals[0].status, "expired");
    }

    #[test]
    fn a_reset_forgets_the_previous_conversation_approvals() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.set_approvals(vec![Approval {
            id: "apr_01J2".to_string(),
            ..Approval::default()
        }]);
        feed.reset("cnv_2");
        assert!(feed.snapshot().approvals.is_empty());
    }

    #[test]
    fn one_card_per_delegation_and_the_newest_state_wins() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_timeline([card(5, "dlg_1", "running")]);
        feed.merge_timeline([card(5, "dlg_1", "done")]);
        let snapshot = feed.snapshot();
        assert_eq!(snapshot.entries.len(), 1, "same seq replaces the card");
        match &snapshot.entries[0] {
            ChatEntry::Card { entry, .. } => assert_eq!(entry.status.as_deref(), Some("done")),
            other => panic!("esperaba tarjeta, llegó {other:?}"),
        }

        // An older page must not overwrite the terminal state.
        feed.merge_timeline([card(3, "dlg_1", "queued")]);
        let snapshot = feed.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        match &snapshot.entries[0] {
            ChatEntry::Card { seq, entry } => {
                assert_eq!(*seq, 5);
                assert_eq!(entry.status.as_deref(), Some("done"));
            }
            other => panic!("esperaba tarjeta, llegó {other:?}"),
        }

        // A newer seq for the same delegation replaces it, still one card.
        feed.merge_timeline([card(7, "dlg_1", "failed")]);
        let snapshot = feed.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        match &snapshot.entries[0] {
            ChatEntry::Card { seq, entry } => {
                assert_eq!(*seq, 7);
                assert_eq!(entry.status.as_deref(), Some("failed"));
            }
            other => panic!("esperaba tarjeta, llegó {other:?}"),
        }

        feed.merge_timeline([card(4, "dlg_2", "running")]);
        assert_eq!(feed.snapshot().entries.len(), 2);
    }

    #[test]
    fn reopening_the_same_conversation_keeps_the_page_but_another_one_resets() {
        let mut feed = ChatFeed::default();
        feed.reset("cnv_1");
        feed.merge_messages([message(1, "user", "hola")]);
        feed.ensure("cnv_1");
        assert_eq!(feed.snapshot().entries.len(), 1);
        feed.ensure("cnv_2");
        assert_eq!(feed.snapshot().entries.len(), 0);
        assert_eq!(feed.conversation_id(), Some("cnv_2"));
    }
}
