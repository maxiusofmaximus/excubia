//! The chat timeline reduction: `seq` is the order and the identity, the raw
//! message sync fills the bodies, and an outgoing message is keyed by
//! `client_id` until the host answers with its `seq`.
//!
//! Dart never merges pages or SSE frames itself: it feeds raw JSON here and
//! renders the snapshot. The host remains the only writer of the truth.

use std::collections::BTreeMap;

use serde_json::Value;

use super::model::{entry_from, message_entry_from, parse_chat_event_value, ChatEntry, ChatEvent};

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
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_chat_reducer() -> ChatReducer {
    ChatReducer {
        entries: BTreeMap::new(),
        outgoing: Vec::new(),
        next_before: None,
    }
}

impl ChatReducer {
    #[flutter_rust_bridge::frb(sync)]
    pub fn reset(&mut self) {
        self.entries.clear();
        self.outgoing.clear();
        self.next_before = None;
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
        let Some(entry) = message_entry_from(message) else {
            return false;
        };
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
        if event.event == "chat.message" && event.change.as_deref() == Some("updated") {
            if let (Some(message_id), Some(state)) = (&event.message_id, &event.state) {
                if let Some(entry) = self
                    .entries
                    .values_mut()
                    .find(|entry| entry.message_id.as_ref() == Some(message_id))
                {
                    entry.state = Some(state.clone());
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

    fn upsert(&mut self, next: ChatEntry) {
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
                self.entries.insert(next.seq, next);
            }
        }
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
        title: None,
        summary: None,
        status: None,
        approval_id: None,
        level: None,
        decider_kind: None,
        decider_id: None,
        created_at: String::new(),
        local: true,
    }
}

fn object(json: &str) -> Option<Value> {
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
