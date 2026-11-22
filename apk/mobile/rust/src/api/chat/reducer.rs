//! The chat timeline reduction: `seq` is the order and the identity, the raw
//! message sync fills the bodies, and an outgoing message is keyed by
//! `client_id` until the host answers with its `seq`.
//!
//! Dart never merges pages or SSE frames itself: it feeds raw JSON here and
//! renders the snapshot. The host remains the only writer of the truth.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::model::{
    entry_from, message_entry_from, parse_chat_event_value, text, ChatConversation, ChatEntry,
    ChatEvent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSnapshot {
    pub entries: Vec<ChatEntry>,
    pub oldest_seq: Option<i64>,
    pub last_seq: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
struct Outgoing {
    client_id: String,
    text: String,
    failed: bool,
}

#[flutter_rust_bridge::frb(opaque)]
pub struct ChatReducer {
    entries: BTreeMap<i64, ChatEntry>,
    outgoing: Vec<Outgoing>,
    next_before: Option<i64>,
    /// Turn states from frames that beat their message into the timeline: the
    /// `updated` frame of a new message can arrive before its `202`.
    early_turns: BTreeMap<String, String>,
    /// The step line of each conversation. It outlives `reset`: the history
    /// shows it for conversations that are not open.
    progress: BTreeMap<String, ProgressLine>,
    /// Conversations a listing has delivered at least once.
    listed: BTreeSet<String>,
}

/// One conversation's step line. `revision` orders it: the host numbers its
/// frames in journal order, so an older run's frame carries a lower one.
#[derive(Debug, Clone)]
struct ProgressLine {
    revision: i64,
    text: Option<String>,
}

/// The revision of a line that came from a listing, below any frame's.
const LISTED: i64 = -1;

impl ProgressLine {
    fn listed() -> Self {
        Self {
            revision: LISTED,
            text: None,
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_chat_reducer() -> ChatReducer {
    ChatReducer {
        entries: BTreeMap::new(),
        outgoing: Vec::new(),
        next_before: None,
        early_turns: BTreeMap::new(),
        progress: BTreeMap::new(),
        listed: BTreeSet::new(),
    }
}

impl ChatReducer {
    #[flutter_rust_bridge::frb(sync)]
    pub fn reset(&mut self) {
        self.entries.clear();
        self.outgoing.clear();
        self.next_before = None;
        self.early_turns.clear();
    }

    /// Merge one `/timeline` page. Entries with a known `seq` are updated, not
    /// duplicated; a body already fetched from `/messages` survives.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_timeline(&mut self, json: String) -> bool {
        let Some(value) = object(&json) else {
            return false;
        };
        let Some(entries) = value.get("entries").and_then(Value::as_array) else {
            return false;
        };
        for raw in entries {
            if let Some(entry) = entry_from(raw) {
                self.upsert(entry);
            }
        }
        self.next_before = value.get("next_before_seq").and_then(Value::as_i64);
        true
    }

    /// Merge `/messages`: full bodies and the current state of each message.
    /// Messages newer than the loaded timeline add their own row.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_messages(&mut self, json: String) -> bool {
        let Some(value) = object(&json) else {
            return false;
        };
        let Some(messages) = value.get("messages").and_then(Value::as_array) else {
            return false;
        };
        for raw in messages {
            if let Some(entry) = message_entry_from(raw) {
                self.upsert(entry);
            }
        }
        true
    }

    /// The `202` answer. Its `client_id` closes the local bubble; the `seq` is
    /// the only place the message is stored, so a replay cannot duplicate it.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_sent(&mut self, client_id: String, json: String) -> bool {
        let Some(value) = object(&json) else {
            return false;
        };
        let Some(message) = value.get("message") else {
            return false;
        };
        let Some(mut entry) = message_entry_from(message) else {
            return false;
        };
        // The 202 carries the new turn beside the message; it is `queued` by contract.
        if entry.turn_state.is_none() {
            entry.turn_state = value.get("turn").and_then(|turn| text(turn, "state"));
        }
        self.outgoing
            .retain(|outgoing| outgoing.client_id != client_id);
        self.upsert(entry);
        true
    }

    /// Add or refresh the optimistic user bubble. Calling it twice with the
    /// same `client_id` is a retry, not a second message.
    #[flutter_rust_bridge::frb(sync)]
    pub fn begin_send(&mut self, client_id: String, text: String) -> bool {
        if client_id.is_empty() {
            return false;
        }
        if let Some(outgoing) = self
            .outgoing
            .iter_mut()
            .find(|outgoing| outgoing.client_id == client_id)
        {
            outgoing.text = text;
            outgoing.failed = false;
            return true;
        }
        self.outgoing.push(Outgoing {
            client_id,
            text,
            failed: false,
        });
        true
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn fail_send(&mut self, client_id: String) {
        if let Some(outgoing) = self
            .outgoing
            .iter_mut()
            .find(|outgoing| outgoing.client_id == client_id)
        {
            outgoing.failed = true;
        }
    }

    /// Parse a `chat.*` frame and apply the facts a body-less frame can carry:
    /// the state of a message already in the timeline. New rows still need a
    /// timeline refresh, which the controller does.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_event(&mut self, json: String) -> Option<ChatEvent> {
        let value: Value = serde_json::from_str(&json).ok()?;
        let event = parse_chat_event_value(&value)?;
        self.apply_event(&event);
        Some(event)
    }

    /// The same reduction for a frame already decoded by the stream. Keeping
    /// one `apply_event` means the socket path and the JSON path cannot drift.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_chat_event(&mut self, event: ChatEvent) -> bool {
        self.apply_event(&event);
        true
    }

    /// A conversation listing. An open turn without a line in the listing clears
    /// the line only on a repeat: a frame that beat the first listing is kept.
    /// A closed turn clears it at once.
    #[flutter_rust_bridge::frb(sync)]
    pub fn accept_conversations(&mut self, conversations: Vec<ChatConversation>) {
        for conversation in conversations {
            let first = self.listed.insert(conversation.id.clone());
            let open = conversation.active_runs > 0;
            let line = self
                .progress
                .entry(conversation.id)
                .or_insert_with(ProgressLine::listed);
            match (open, conversation.progress) {
                (false, _) => line.text = None,
                (true, Some(text)) if line.revision == LISTED => line.text = Some(text),
                (true, Some(_)) => {}
                (true, None) if first && line.revision != LISTED => {}
                (true, None) => line.text = None,
            }
        }
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn progress_of(&self, conversation_id: String) -> Option<String> {
        self.progress.get(&conversation_id)?.text.clone()
    }

    /// Clears the line once the timeline shows the turn over. It covers a lost
    /// `chat.progress` null frame; a send still on its way does not count.
    #[flutter_rust_bridge::frb(sync)]
    pub fn settle_turn(&mut self, conversation_id: String) {
        if !self.turn_finished() {
            return;
        }
        if let Some(line) = self.progress.get_mut(&conversation_id) {
            line.text = None;
        }
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn discard_send(&mut self, client_id: String) {
        self.outgoing
            .retain(|outgoing| outgoing.client_id != client_id);
    }

    /// Apply the answer of `POST …/cancel` without waiting for the SSE frame.
    /// The card keeps its `seq` and history; only its state changes.
    #[flutter_rust_bridge::frb(sync)]
    pub fn update_card(&mut self, delegation_id: String, status: String, summary: String) -> bool {
        let Some(entry) = self
            .entries
            .values_mut()
            .find(|entry| entry.delegation_id.as_ref() == Some(&delegation_id))
        else {
            return false;
        };
        set_card_status(entry, status);
        if !summary.is_empty() {
            entry.summary = Some(summary);
        }
        true
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn snapshot(&self) -> ChatSnapshot {
        let mut entries: Vec<ChatEntry> = self.entries.values().cloned().collect();
        entries.extend(self.outgoing.iter().map(outgoing_entry));
        ChatSnapshot {
            entries,
            oldest_seq: self.entries.keys().next().copied(),
            last_seq: self.entries.keys().next_back().copied().unwrap_or(0),
            has_more: self.next_before.is_some(),
        }
    }

    fn apply_event(&mut self, event: &ChatEvent) {
        if event.event == "chat.progress" {
            self.move_progress(
                &event.conversation_id,
                event.revision,
                event.progress.clone(),
            );
            return;
        }
        if event.event == "chat.message" && event.change.as_deref() == Some("updated") {
            // A turn that ends closes its line even when the null frame was lost.
            if event
                .turn_state
                .as_deref()
                .is_some_and(|state| turn_rank(state) == 3)
            {
                self.move_progress(&event.conversation_id, event.revision, None);
            }
            // A turn frame carries `turn_state` and no `state`; a message frame the
            // reverse. Each sets only what it carries.
            if let Some(message_id) = &event.message_id {
                match self
                    .entries
                    .values_mut()
                    .find(|entry| entry.message_id.as_ref() == Some(message_id))
                {
                    Some(entry) => {
                        if let Some(state) = &event.state {
                            entry.state = Some(state.clone());
                        }
                        if let Some(turn_state) = &event.turn_state {
                            advance_turn(&mut entry.turn_state, turn_state);
                        }
                    }
                    None => {
                        if let Some(turn_state) = &event.turn_state {
                            let early = self.early_turns.entry(message_id.clone()).or_default();
                            if turn_rank(turn_state) >= turn_rank(early) {
                                early.clone_from(turn_state);
                            }
                        }
                    }
                }
            }
            return;
        }
        if !matches!(event.event.as_str(), "chat.delegation" | "chat.activity") {
            return;
        }
        let Some(entry) = self.entries.values_mut().find(|entry| {
            (event.delegation_id.is_some() && entry.delegation_id == event.delegation_id)
                || (event.card_id.is_some() && entry.card_id == event.card_id)
        }) else {
            return;
        };
        if let Some(status) = &event.status {
            set_card_status(entry, status.clone());
        }
        if let Some(summary) = &event.summary {
            entry.summary = Some(summary.clone());
        }
        if event.pane_id.is_some() {
            entry.pane_id = event.pane_id.clone();
        }
    }

    /// A frame moves the line only when it is not older than the one held, so a
    /// finished run's late frame cannot blank the step of the run after it.
    fn move_progress(&mut self, conversation_id: &str, revision: i64, text: Option<String>) {
        let line = self
            .progress
            .entry(conversation_id.to_string())
            .or_insert_with(ProgressLine::listed);
        if revision >= line.revision {
            *line = ProgressLine { revision, text };
        }
    }

    /// The newest own message has a terminal turn, or an agent message came after
    /// it. A send still on its way is not a finished turn.
    fn turn_finished(&self) -> bool {
        if self.outgoing.iter().any(|outgoing| !outgoing.failed) {
            return false;
        }
        let is_kind = |entry: &ChatEntry, author: &str| {
            entry.kind == "message" && entry.author_kind.as_deref() == Some(author)
        };
        let Some(own) = self
            .entries
            .values()
            .rev()
            .find(|entry| is_kind(entry, "user"))
        else {
            return false;
        };
        own.turn_state
            .as_deref()
            .is_some_and(|state| turn_rank(state) == 3)
            || self
                .entries
                .values()
                .any(|entry| entry.seq > own.seq && is_kind(entry, "agent"))
    }

    fn upsert(&mut self, mut next: ChatEntry) {
        // The host owns exactly one card per delegation. A timeline refresh can
        // repeat it with a newer state, so merge by `delegation_id` instead of
        // appending a second card.
        if next.kind == "card" {
            if let Some(delegation_id) = next.delegation_id.as_deref() {
                if let Some(existing) = self.entries.values_mut().find(|entry| {
                    entry.kind == "card" && entry.delegation_id.as_deref() == Some(delegation_id)
                }) {
                    merge_card(existing, next);
                    return;
                }
            }
        }
        match self.entries.get_mut(&next.seq) {
            Some(existing) => {
                if !next.body.is_empty() {
                    existing.body = next.body;
                }
                if !next.preview.is_empty() {
                    existing.preview = next.preview;
                }
                existing.kind = next.kind;
                if next.message_id.is_some() {
                    existing.message_id = next.message_id;
                }
                if next.card_id.is_some() {
                    existing.card_id = next.card_id;
                }
                if next.delegation_id.is_some() {
                    existing.delegation_id = next.delegation_id;
                }
                if next.agent_id.is_some() {
                    existing.agent_id = next.agent_id;
                }
                if next.pane_id.is_some() {
                    existing.pane_id = next.pane_id;
                }
                if next.attention.is_some() {
                    existing.attention = next.attention;
                }
                if next.agent_link.is_some() {
                    existing.agent_link = next.agent_link;
                }
                if next.delegation_link.is_some() {
                    existing.delegation_link = next.delegation_link;
                }
                if next.author_kind.is_some() {
                    existing.author_kind = next.author_kind;
                }
                if next.author_agent_id.is_some() {
                    existing.author_agent_id = next.author_agent_id;
                }
                if next.state.is_some() {
                    existing.state = next.state;
                }
                if let Some(turn_state) = &next.turn_state {
                    advance_turn(&mut existing.turn_state, turn_state);
                }
                // A message sync carries neither field; only the timeline sets them.
                if !next.suggestions.is_empty() {
                    existing.suggestions = next.suggestions;
                }
                if next.quote.is_some() {
                    existing.quote = next.quote;
                }
                if next.attachments.is_some() {
                    existing.attachments = next.attachments;
                }
                if next.title.is_some() {
                    existing.title = next.title;
                }
                if next.summary.is_some() {
                    existing.summary = next.summary;
                }
                if next.status.is_some() {
                    existing.status = next.status;
                }
                if existing.created_at.is_empty() {
                    existing.created_at = next.created_at;
                }
            }
            None => {
                if let Some(early) = next
                    .message_id
                    .as_ref()
                    .and_then(|id| self.early_turns.get(id))
                {
                    advance_turn(&mut next.turn_state, early);
                }
                self.entries.insert(next.seq, next);
            }
        }
    }
}

/// The order a turn takes its states in. A later rank is never replaced by an
/// earlier one, so a stale timeline or the `202` cannot reopen a finished turn.
fn turn_rank(state: &str) -> u8 {
    match state {
        "queued" => 1,
        "running" => 2,
        "completed" | "failed" | "interrupted" | "stopped" => 3,
        _ => 0,
    }
}

fn advance_turn(current: &mut Option<String>, next: &str) {
    let ahead = match current.as_deref() {
        Some(now) => turn_rank(next) >= turn_rank(now),
        None => true,
    };
    if ahead {
        *current = Some(next.to_string());
    }
}

fn merge_card(existing: &mut ChatEntry, next: ChatEntry) {
    if !next.preview.is_empty() {
        existing.preview = next.preview;
    }
    if next.card_id.is_some() {
        existing.card_id = next.card_id;
    }
    if next.agent_id.is_some() {
        existing.agent_id = next.agent_id;
    }
    if next.pane_id.is_some() {
        existing.pane_id = next.pane_id;
    }
    if next.attention.is_some() {
        existing.attention = next.attention;
    }
    if next.agent_link.is_some() {
        existing.agent_link = next.agent_link;
    }
    if next.delegation_link.is_some() {
        existing.delegation_link = next.delegation_link;
    }
    if next.title.is_some() {
        existing.title = next.title;
    }
    if next.summary.is_some() {
        existing.summary = next.summary;
    }
    if let Some(status) = next.status {
        set_card_status(existing, status);
    }
    if existing.created_at.is_empty() {
        existing.created_at = next.created_at;
    }
}

/// A terminal card never goes back to a live state because of a late frame;
/// the latest terminal state (`done`, `failed`, `cancelled`, `interrupted`)
/// still wins.
fn set_card_status(entry: &mut ChatEntry, status: String) {
    let current = entry.status.as_deref();
    if current.is_some_and(terminal_card_state) && !terminal_card_state(&status) {
        return;
    }
    entry.status = Some(status.clone());
    entry.state = Some(status);
}

fn terminal_card_state(state: &str) -> bool {
    matches!(state, "done" | "failed" | "cancelled" | "interrupted")
}

fn outgoing_entry(outgoing: &Outgoing) -> ChatEntry {
    ChatEntry {
        seq: 0,
        kind: "message".to_string(),
        message_id: None,
        card_id: None,
        delegation_id: None,
        client_id: Some(outgoing.client_id.clone()),
        author_kind: Some("user".to_string()),
        author_agent_id: None,
        agent_id: None,
        pane_id: None,
        attention: None,
        agent_link: None,
        delegation_link: None,
        body: outgoing.text.clone(),
        preview: super::model::preview(&outgoing.text),
        state: Some(if outgoing.failed {
            "failed".to_string()
        } else {
            "sending".to_string()
        }),
        turn_state: None,
        title: None,
        summary: None,
        status: None,
        approval_id: None,
        level: None,
        decider_kind: None,
        decider_id: None,
        mail: None,
        mail_draft: None,
        suggestions: Vec::new(),
        quote: None,
        attachments: None,
        created_at: String::new(),
        local: true,
    }
}

pub(crate) fn object(json: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline(entries: &str, next: Option<i64>) -> String {
        match next {
            Some(seq) => format!(r#"{{"ok":true,"entries":[{entries}],"next_before_seq":{seq}}}"#),
            None => format!(r#"{{"ok":true,"entries":[{entries}]}}"#),
        }
    }

    fn message(seq: i64, id: &str, body: &str, state: &str) -> String {
        format!(
            r#"{{"id":"{id}","seq":{seq},"author_kind":"agent",
                "body_md":"{body}","state":"{state}","created_at":"t"}}"#
        )
    }

    #[test]
    fn a_later_body_keeps_the_suggestions_and_the_quote() {
        let mut reducer = new_chat_reducer();
        assert!(reducer.accept_timeline(timeline(
            r#"{"seq":1,"kind":"message","message_id":"m1","author_kind":"agent",
                "suggestions":["Sí, créalo"]},
               {"seq":2,"kind":"message","message_id":"m2","author_kind":"user",
                "quote":{"kind":"message","id":"m1","author_kind":"agent","preview":"¿Qué nombre?"}}"#,
            None
        )));
        assert!(reducer.accept_messages(format!(
            r#"{{"ok":true,"messages":[{},{}]}}"#,
            message(1, "m1", "¿Qué nombre?", "complete"),
            r#"{"id":"m2","seq":2,"author_kind":"user","body_md":"Sí","state":"complete","created_at":"t"}"#,
        )));

        let snapshot = reducer.snapshot();
        assert_eq!(snapshot.entries[0].body, "¿Qué nombre?");
        assert_eq!(snapshot.entries[0].suggestions, ["Sí, créalo"]);
        assert_eq!(
            snapshot.entries[1].quote.as_ref().unwrap().preview,
            "¿Qué nombre?"
        );
    }

    #[test]
    fn entries_are_ordered_by_seq_whatever_the_arrival_order() {
        let mut reducer = new_chat_reducer();
        assert!(reducer.accept_timeline(timeline(
            r#"{"seq":5,"kind":"message","message_id":"m5"},
               {"seq":2,"kind":"message","message_id":"m2"}"#,
            Some(2)
        )));
        assert!(reducer.accept_timeline(timeline(
            r#"{"seq":3,"kind":"card","title":"Alfred pidió a Ernesto"},
               {"seq":2,"kind":"message","message_id":"m2","preview":"dos"}"#,
            None
        )));
        let snapshot = reducer.snapshot();
        let seqs: Vec<i64> = snapshot.entries.iter().map(|entry| entry.seq).collect();
        assert_eq!(seqs, vec![2, 3, 5]);
        assert_eq!(snapshot.entries[1].kind, "card");
        assert_eq!(snapshot.entries[0].preview, "dos");
        assert!(!snapshot.has_more, "the newest page has no older cursor");
    }

    #[test]
    fn a_message_body_fills_its_timeline_row_and_survives_a_later_page() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":4,"kind":"message","message_id":"m4","preview":"hola"}"#,
            None,
        ));
        reducer.accept_messages(format!(
            r#"{{"ok":true,"messages":[{}]}}"#,
            message(4, "m4", "hola completo", "complete")
        ));
        reducer.accept_timeline(timeline(
            r#"{"seq":4,"kind":"message","message_id":"m4","preview":"hola",
                "state":"failed"}"#,
            None,
        ));
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.body, "hola completo");
        assert_eq!(entry.state.as_deref(), Some("failed"));
    }

    #[test]
    fn a_duplicate_client_id_is_one_bubble() {
        let mut reducer = new_chat_reducer();
        assert!(reducer.begin_send("c1".into(), "hola".into()));
        assert!(reducer.begin_send("c1".into(), "hola".into()));
        assert_eq!(reducer.snapshot().entries.len(), 1);
        assert!(reducer.snapshot().entries[0].local);

        let accepted = format!(
            r#"{{"ok":true,"message":{},"turn":{{"id":"run_1","state":"queued","agent_id":"ag_1"}}}}"#,
            message(1, "msg_1", "hola", "complete")
        );
        assert!(reducer.accept_sent("c1".into(), accepted.clone()));
        assert!(reducer.accept_sent("c1".into(), accepted));
        reducer.accept_timeline(timeline(
            r#"{"seq":1,"kind":"message","message_id":"msg_1","preview":"hola"}"#,
            None,
        ));
        let snapshot = reducer.snapshot();
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.entries[0].local);
        assert_eq!(snapshot.entries[0].body, "hola");
    }

    #[test]
    fn a_failed_local_send_keeps_its_text_and_client_id() {
        let mut reducer = new_chat_reducer();
        reducer.begin_send("c1".into(), "texto".into());
        reducer.fail_send("c1".into());
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.state.as_deref(), Some("failed"));
        assert_eq!(entry.body, "texto");
        assert_eq!(entry.client_id.as_deref(), Some("c1"));

        reducer.discard_send("c1".into());
        assert!(reducer.snapshot().entries.is_empty());
    }

    #[test]
    fn older_pages_move_the_cursor_and_are_deduplicated() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":10,"kind":"message","message_id":"m10"}"#,
            Some(10),
        ));
        assert!(reducer.snapshot().has_more);
        reducer.accept_timeline(timeline(
            r#"{"seq":9,"kind":"message","message_id":"m9"},
               {"seq":10,"kind":"message","message_id":"m10"}"#,
            Some(9),
        ));
        let snapshot = reducer.snapshot();
        assert_eq!(snapshot.oldest_seq, Some(9));
        assert_eq!(snapshot.entries.len(), 2);
        assert!(snapshot.has_more);
    }

    #[test]
    fn unknown_kinds_are_kept_as_neutral_entries() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":7,"kind":"hologram","title":"Futuro","summary":"algo"}"#,
            None,
        ));
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.kind, "hologram");
        assert_eq!(entry.title.as_deref(), Some("Futuro"));
    }

    #[test]
    fn an_updated_frame_changes_the_state_without_inventing_a_body() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":2,"kind":"message","message_id":"m2","state":"pending"}"#,
            None,
        ));
        let event = reducer
            .accept_event(
                r#"{"event":"chat.message","revision":9,"conversation_id":"cnv_1",
                    "message_id":"m2","seq":2,"change":"updated","state":"failed"}"#
                    .to_string(),
            )
            .unwrap();
        assert_eq!(event.seq, Some(2));
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.state.as_deref(), Some("failed"));
        assert_eq!(entry.body, "");
    }

    fn listing(id: &str, active_runs: u32, progress: Option<&str>) -> ChatConversation {
        let progress = progress.map_or("null".to_string(), |text| format!("\"{text}\""));
        super::super::model::parse_chat_conversation(format!(
            r#"{{"ok":true,"conversation":{{"id":"{id}","active_runs":{active_runs},"progress":{progress},"created_at":"a"}}}}"#
        ))
        .unwrap()
    }

    fn progress_frame(conversation: &str, revision: i64, text: Option<&str>) -> String {
        let text = text.map_or("null".to_string(), |text| format!("\"{text}\""));
        format!(
            r#"{{"event":"chat.progress","conversation_id":"{conversation}","run_id":"run_1","revision":{revision},"text":{text}}}"#
        )
    }

    #[test]
    fn a_step_line_sets_and_a_null_frame_clears_it() {
        let mut reducer = new_chat_reducer();
        reducer.accept_event(progress_frame("cnv_1", 5, Some("Revisando los borradores")));
        assert_eq!(
            reducer.progress_of("cnv_1".to_string()).as_deref(),
            Some("Revisando los borradores")
        );
        reducer.accept_event(progress_frame("cnv_1", 6, None));
        assert_eq!(reducer.progress_of("cnv_1".to_string()), None);
    }

    #[test]
    fn an_older_run_never_overwrites_a_newer_one() {
        let mut reducer = new_chat_reducer();
        reducer.accept_event(progress_frame("cnv_1", 9, Some("Run nuevo")));
        reducer.accept_event(progress_frame("cnv_1", 7, Some("Run viejo")));
        reducer.accept_event(progress_frame("cnv_1", 8, None));
        assert_eq!(
            reducer.progress_of("cnv_1".to_string()).as_deref(),
            Some("Run nuevo")
        );
    }

    #[test]
    fn a_frame_that_beats_its_conversation_is_kept_for_the_first_listing() {
        let mut reducer = new_chat_reducer();
        reducer.accept_event(progress_frame("cnv_2", 4, Some("Pensando")));
        reducer.accept_conversations(vec![listing("cnv_2", 1, None)]);
        assert_eq!(
            reducer.progress_of("cnv_2".to_string()).as_deref(),
            Some("Pensando")
        );
        reducer.accept_conversations(vec![listing("cnv_2", 1, None)]);
        assert_eq!(reducer.progress_of("cnv_2".to_string()), None);
    }

    #[test]
    fn a_closed_turn_clears_its_line_without_the_null_frame() {
        let mut reducer = new_chat_reducer();
        reducer.accept_conversations(vec![listing("cnv_1", 1, Some("Revisando"))]);
        assert_eq!(
            reducer.progress_of("cnv_1".to_string()).as_deref(),
            Some("Revisando")
        );
        reducer.accept_conversations(vec![listing("cnv_1", 0, None)]);
        assert_eq!(reducer.progress_of("cnv_1".to_string()), None);

        reducer.accept_event(progress_frame("cnv_1", 10, Some("Redactando")));
        reducer.accept_event(
            r#"{"event":"chat.message","conversation_id":"cnv_1","revision":11,
                "change":"updated","message_id":"m1","turn_state":"completed"}"#
                .to_string(),
        );
        assert_eq!(reducer.progress_of("cnv_1".to_string()), None);

        reducer.accept_event(progress_frame("cnv_1", 12, Some("Otra vez")));
        let own = r#"{"seq":3,"kind":"message","message_id":"m3","author_kind":"user",
            "turn_state":"running"}"#;
        reducer.accept_timeline(timeline(own, None));
        reducer.settle_turn("cnv_1".to_string());
        assert_eq!(
            reducer.progress_of("cnv_1".to_string()).as_deref(),
            Some("Otra vez"),
            "a turn still running keeps its line"
        );
        let reply = r#"{"seq":4,"kind":"message","message_id":"m4","author_kind":"agent"}"#;
        reducer.accept_timeline(timeline(&format!("{own},{reply}"), None));
        reducer.settle_turn("cnv_1".to_string());
        assert_eq!(reducer.progress_of("cnv_1".to_string()), None);
    }

    #[test]
    fn a_turn_frame_sets_turn_state_and_keeps_the_message_state() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":2,"kind":"message","message_id":"m2","turn_state":"queued"}"#,
            None,
        ));
        reducer.accept_event(
            r#"{"event":"chat.message","revision":10,"conversation_id":"cnv_1",
                "message_id":"m2","seq":2,"change":"updated","turn_state":"running"}"#
                .to_string(),
        );
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.turn_state.as_deref(), Some("running"));
        assert_eq!(entry.state, None);
    }

    #[test]
    fn the_202_turn_marks_the_sent_message_queued() {
        let mut reducer = new_chat_reducer();
        reducer.begin_send("c1".into(), "hola".into());
        let accepted = format!(
            r#"{{"ok":true,"message":{},"turn":{{"id":"run_1","state":"queued","agent_id":"ag_1"}}}}"#,
            message(1, "msg_1", "hola", "complete")
        );
        assert!(reducer.accept_sent("c1".into(), accepted));
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.turn_state.as_deref(), Some("queued"));
        assert!(!entry.local);
    }

    #[test]
    fn a_turn_that_ends_before_the_202_is_not_reopened_by_it() {
        let mut reducer = new_chat_reducer();
        reducer.begin_send("c1".into(), "hola".into());
        for state in ["running", "completed"] {
            reducer.accept_event(format!(
                r#"{{"event":"chat.message","revision":11,"conversation_id":"cnv_1",
                    "message_id":"msg_1","seq":1,"change":"updated","turn_state":"{state}"}}"#
            ));
        }
        let accepted = format!(
            r#"{{"ok":true,"message":{},"turn":{{"id":"run_1","state":"queued","agent_id":"ag_1"}}}}"#,
            message(1, "msg_1", "hola", "complete")
        );
        assert!(reducer.accept_sent("c1".into(), accepted));
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.turn_state.as_deref(), Some("completed"));
    }

    #[test]
    fn a_turn_that_ends_after_the_202_is_seen_through_the_frames() {
        let mut reducer = new_chat_reducer();
        reducer.begin_send("c1".into(), "hola".into());
        let accepted = format!(
            r#"{{"ok":true,"message":{},"turn":{{"id":"run_1","state":"queued","agent_id":"ag_1"}}}}"#,
            message(1, "msg_1", "hola", "complete")
        );
        assert!(reducer.accept_sent("c1".into(), accepted));
        for state in ["running", "completed"] {
            reducer.accept_event(format!(
                r#"{{"event":"chat.message","revision":11,"conversation_id":"cnv_1",
                    "message_id":"msg_1","seq":1,"change":"updated","turn_state":"{state}"}}"#
            ));
        }
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.turn_state.as_deref(), Some("completed"));
    }

    #[test]
    fn a_stale_timeline_or_frame_never_moves_a_turn_backwards() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":2,"kind":"message","message_id":"m2","turn_state":"completed"}"#,
            None,
        ));
        reducer.accept_timeline(timeline(
            r#"{"seq":2,"kind":"message","message_id":"m2","turn_state":"queued"}"#,
            None,
        ));
        reducer.accept_event(
            r#"{"event":"chat.message","revision":12,"conversation_id":"cnv_1",
                "message_id":"m2","seq":2,"change":"updated","turn_state":"running"}"#
                .to_string(),
        );
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.turn_state.as_deref(), Some("completed"));
    }

    fn card(seq: i64, delegation: &str, status: &str, summary: &str, pane: &str) -> String {
        format!(
            r#"{{"seq":{seq},"kind":"card","card_id":"act_{delegation}",
                "delegation_id":"{delegation}","ref_id":"{delegation}",
                "title":"Alfred pidió a Ernesto","summary":"{summary}",
                "status":"{status}","pane_id":"{pane}","created_at":"t"}}"#
        )
    }

    #[test]
    fn one_card_per_delegation_keeps_the_latest_state() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            &card(3, "dlg_1", "running", "Ernesto trabaja", "w1:p3"),
            None,
        ));
        reducer.accept_timeline(timeline(
            &card(3, "dlg_1", "done", "Ernesto terminó", "w1:p3"),
            None,
        ));
        let snapshot = reducer.snapshot();
        assert_eq!(
            snapshot.entries.len(),
            1,
            "the refresh must not append a card"
        );
        let entry = &snapshot.entries[0];
        assert_eq!(entry.status.as_deref(), Some("done"));
        assert_eq!(entry.summary.as_deref(), Some("Ernesto terminó"));
        assert_eq!(entry.delegation_id.as_deref(), Some("dlg_1"));
    }

    #[test]
    fn delegation_frames_update_the_card_without_polling() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            &card(3, "dlg_1", "queued", "En cola", "w1:p3"),
            None,
        ));
        reducer
            .accept_event(
                r#"{"event":"chat.delegation","revision":4,"conversation_id":"cnv_1",
                    "delegation_id":"dlg_1","card_id":"act_dlg_1","status":"running",
                    "summary":"Ernesto trabaja","pane_id":"w1:p4"}"#
                    .to_string(),
            )
            .unwrap();
        let entry = &reducer.snapshot().entries[0];
        assert_eq!(entry.status.as_deref(), Some("running"));
        assert_eq!(entry.pane_id.as_deref(), Some("w1:p4"));
        assert_eq!(entry.summary.as_deref(), Some("Ernesto trabaja"));
    }

    #[test]
    fn a_late_live_frame_does_not_reopen_a_terminal_card() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(&card(3, "dlg_1", "done", "Hecho", "w1:p3"), None));
        reducer
            .accept_event(
                r#"{"event":"chat.activity","revision":5,"conversation_id":"cnv_1",
                    "delegation_id":"dlg_1","status":"running"}"#
                    .to_string(),
            )
            .unwrap();
        assert_eq!(
            reducer.snapshot().entries[0].status.as_deref(),
            Some("done")
        );
        assert!(reducer.update_card(
            "dlg_1".into(),
            "cancelled".into(),
            "Cancelada por ti".into()
        ));
        assert_eq!(
            reducer.snapshot().entries[0].status.as_deref(),
            Some("cancelled")
        );
    }

    #[test]
    fn malformed_pages_are_ignored_without_emptying_the_reducer() {
        let mut reducer = new_chat_reducer();
        reducer.accept_timeline(timeline(
            r#"{"seq":1,"kind":"message","message_id":"m1"}"#,
            None,
        ));
        assert!(!reducer.accept_timeline("not json".into()));
        assert!(!reducer.accept_timeline(r#"{"ok":false,"entries":[]}"#.into()));
        assert_eq!(reducer.snapshot().entries.len(), 1);
    }
}
