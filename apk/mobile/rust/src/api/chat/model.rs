//! Wire-facing chat models and lenient decoders (`docs/contracts/api.md`,
//! `/v1/chat`). Unknown fields are ignored; unknown timeline kinds are kept as
//! a neutral entry so an older phone never breaks on a newer card.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub pinned: bool,
    pub section_id: Option<String>,
    pub archived: bool,
    pub last_message_at: String,
    pub last_seq: i64,
    pub last_read_seq: i64,
    pub unread: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatConversationPage {
    pub conversations: Vec<ChatConversation>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatBootstrap {
    pub alfred_name: String,
    pub alfred_status: String,
    pub unread_conversations: u64,
    pub unread_messages: u64,
    pub pending_approvals: u64,
    pub revision: i64,
    pub projects: Vec<super::model_team::ChatProject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSection {
    pub id: String,
    pub name: String,
    pub order: i64,
}

/// One row of the timeline as the screen renders it. `body` is filled from the
/// raw message sync; `preview` is the bounded one-line contract field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatEntry {
    pub seq: i64,
    pub kind: String,
    pub message_id: Option<String>,
    pub card_id: Option<String>,
    pub delegation_id: Option<String>,
    pub client_id: Option<String>,
    pub author_kind: Option<String>,
    pub author_agent_id: Option<String>,
    pub agent_id: Option<String>,
    pub pane_id: Option<String>,
    pub attention: Option<i64>,
    pub agent_link: Option<String>,
    pub delegation_link: Option<String>,
    pub body: String,
    pub preview: String,
    pub state: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub approval_id: Option<String>,
    pub level: Option<String>,
    pub decider_kind: Option<String>,
    pub decider_id: Option<String>,
    pub created_at: String,
    pub local: bool,
}

/// What one SSE chat frame says. It never carries the body: the controller
/// refreshes the timeline, which is the authoritative view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatEvent {
    pub event: String,
    pub conversation_id: String,
    pub revision: i64,
    pub message_id: Option<String>,
    pub seq: Option<i64>,
    pub change: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
    pub delegation_id: Option<String>,
    pub card_id: Option<String>,
    pub pane_id: Option<String>,
    pub summary: Option<String>,
    pub approval_id: Option<String>,
    pub level: Option<String>,
    pub escalated_from: Option<String>,
    pub expires_at: Option<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_conversations(json: String) -> Option<ChatConversationPage> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let conversations = value
        .get("conversations")?
        .as_array()?
        .iter()
        .filter_map(conversation_from)
        .collect();
    Some(ChatConversationPage {
        conversations,
        next_cursor: value
            .get("next_cursor")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_conversation(json: String) -> Option<ChatConversation> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    conversation_from(value.get("conversation")?)
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_section(json: String) -> Option<ChatSection> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let section = value.get("section")?;
    Some(ChatSection {
        id: section.get("id")?.as_str()?.to_string(),
        name: text(section, "name")?,
        order: section.get("order").and_then(Value::as_i64).unwrap_or(0),
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_delegation(json: String) -> Option<super::model_team::ChatDelegation> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    super::model_team::delegation_from(&value)
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_bootstrap(json: String) -> Option<ChatBootstrap> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let alfred = value.get("alfred")?;
    Some(ChatBootstrap {
        alfred_name: text(alfred, "name").unwrap_or_else(|| "Alfred".to_string()),
        alfred_status: text(alfred, "status").unwrap_or_else(|| "unknown".to_string()),
        unread_conversations: count(value.get("unread"), "conversations"),
        unread_messages: count(value.get("unread"), "messages"),
        pending_approvals: value
            .get("chat")
            .and_then(|chat| chat.get("pending_approvals"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        revision: value
            .get("chat")
            .and_then(|chat| chat.get("revision"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        projects: value
            .get("projects")
            .and_then(Value::as_array)
            .map(|projects| {
                projects
                    .iter()
                    .filter_map(super::model_team::project_from)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_sections(json: String) -> Option<Vec<ChatSection>> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(
        value
            .get("sections")?
            .as_array()?
            .iter()
            .filter_map(|section| {
                Some(ChatSection {
                    id: section.get("id")?.as_str()?.to_string(),
                    name: text(section, "name")?,
                    order: section.get("order").and_then(Value::as_i64).unwrap_or(0),
                })
            })
            .collect(),
    )
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_event(json: String) -> Option<ChatEvent> {
    let value: Value = serde_json::from_str(&json).ok()?;
    parse_chat_event_value(&value)
}

pub(crate) fn parse_chat_event_value(value: &Value) -> Option<ChatEvent> {
    let event = value.get("event")?.as_str()?.to_string();
    if !event.starts_with("chat.") {
        return None;
    }
    let conversation_id = match value.get("conversation_id")? {
        Value::String(id) => id.clone(),
        // An approval with no conversation (inbox only) sends null, not a string.
        Value::Null if event == "chat.approval" => String::new(),
        _ => return None,
    };
    Some(ChatEvent {
        conversation_id,
        revision: value.get("revision").and_then(Value::as_i64).unwrap_or(0),
        message_id: value
            .get("message_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        seq: value.get("seq").and_then(Value::as_i64),
        change: value
            .get("change")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        delegation_id: value
            .get("delegation_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        card_id: value
            .get("card_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        pane_id: value
            .get("pane_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        summary: value
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string),
        approval_id: text(value, "approval_id"),
        level: text(value, "level"),
        escalated_from: text(value, "escalated_from"),
        expires_at: text(value, "expires_at"),
        event,
    })
}

pub(crate) fn conversation_from(value: &Value) -> Option<ChatConversation> {
    let id = text(value, "id")?;
    let created_at = text(value, "created_at").unwrap_or_default();
    Some(ChatConversation {
        id,
        title: text(value, "title").unwrap_or_else(|| "Nuevo chat".to_string()),
        project_id: text(value, "project_id"),
        pinned: value
            .get("pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        section_id: text(value, "section_id"),
        archived: value
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        last_message_at: text(value, "last_message_at").unwrap_or_else(|| created_at.clone()),
        last_seq: value.get("last_seq").and_then(Value::as_i64).unwrap_or(0),
        last_read_seq: value
            .get("last_read_seq")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        unread: value.get("unread").and_then(Value::as_u64).unwrap_or(0),
        created_at,
        updated_at: text(value, "updated_at").unwrap_or_default(),
    })
}

pub(crate) fn entry_from(value: &Value) -> Option<ChatEntry> {
    let seq = value.get("seq").and_then(Value::as_i64)?;
    if seq <= 0 {
        return None;
    }
    let links = value.get("links");
    Some(ChatEntry {
        seq,
        kind: text(value, "kind").unwrap_or_else(|| "unknown".to_string()),
        message_id: text(value, "message_id"),
        card_id: text(value, "card_id"),
        delegation_id: text(value, "delegation_id").or_else(|| text(value, "ref_id")),
        client_id: None,
        author_kind: text(value, "author_kind"),
        author_agent_id: text(value, "author_agent_id"),
        agent_id: text(value, "agent_id"),
        pane_id: text(value, "pane_id"),
        attention: value.get("attention").and_then(Value::as_i64),
        agent_link: links.and_then(|links| text(links, "agent")),
        delegation_link: links.and_then(|links| text(links, "delegation")),
        body: String::new(),
        preview: text(value, "preview").unwrap_or_default(),
        state: text(value, "state"),
        title: text(value, "title"),
        summary: text(value, "summary"),
        status: text(value, "status"),
        approval_id: text(value, "approval_id"),
        level: text(value, "level"),
        decider_kind: text(value, "decider_kind"),
        decider_id: text(value, "decider_id"),
        created_at: text(value, "created_at").unwrap_or_default(),
        local: false,
    })
}

pub(crate) fn message_entry_from(value: &Value) -> Option<ChatEntry> {
    let message_id = text(value, "id")?;
    let seq = value.get("seq").and_then(Value::as_i64)?;
    if seq <= 0 {
        return None;
    }
    let body = text(value, "body_md").unwrap_or_default();
    let preview_text = preview(&body);
    Some(ChatEntry {
        seq,
        kind: "message".to_string(),
        message_id: Some(message_id),
        card_id: None,
        delegation_id: None,
        client_id: None,
        author_kind: text(value, "author_kind"),
        author_agent_id: text(value, "author_agent_id"),
        agent_id: None,
        pane_id: None,
        attention: None,
        agent_link: None,
        delegation_link: None,
        body,
        preview: preview_text,
        state: text(value, "state"),
        title: None,
        summary: None,
        status: None,
        approval_id: None,
        level: None,
        decider_kind: None,
        decider_id: None,
        created_at: text(value, "created_at").unwrap_or_default(),
        local: false,
    })
}

pub(crate) fn preview(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let line = text.lines().next().unwrap_or_default();
    let mut chars = line.chars();
    let mut preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        preview.pop();
        preview.push('…');
    }
    preview
}

pub(crate) fn text(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn count(value: Option<&Value>, field: &str) -> u64 {
    value
        .and_then(|value| value.get(field))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_decode_with_the_contract_defaults() {
        let page = parse_chat_conversations(
            r#"{"ok":true,"conversations":[
                {"id":"cnv_1","title":"Plan","last_seq":6,"last_read_seq":2,"unread":3,
                 "pinned":true,"created_at":"a","updated_at":"b","last_message_at":"c"},
                {"id":"","title":"drop me"}
            ],"next_cursor":"cnv_2"}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(page.conversations.len(), 1);
        assert_eq!(page.conversations[0].last_seq, 6);
        assert!(page.conversations[0].pinned);
        assert_eq!(page.next_cursor.as_deref(), Some("cnv_2"));
    }

    #[test]
    fn bootstrap_carries_alfred_and_the_counters() {
        let boot = parse_chat_bootstrap(
            r#"{"ok":true,"alfred":{"name":"Alfred","status":"working"},
                "unread":{"conversations":2,"messages":5,"approvals":1},
                "chat":{"revision":9,"pending_approvals":1}}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(boot.alfred_name, "Alfred");
        assert_eq!(boot.alfred_status, "working");
        assert_eq!(boot.unread_messages, 5);
        assert_eq!(boot.pending_approvals, 1);
        assert_eq!(boot.revision, 9);
    }

    #[test]
    fn a_chat_frame_carries_ids_not_bodies() {
        let event = parse_chat_event(
            r#"{"event":"chat.message","revision":58,"conversation_id":"cnv_1",
                "message_id":"msg_1","seq":12,"change":"created","author_kind":"user",
                "preview":"hola"}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(event.event, "chat.message");
        assert_eq!(event.conversation_id, "cnv_1");
        assert_eq!(event.seq, Some(12));
        assert!(parse_chat_event(r#"{"event":"state","conversation_id":"x"}"#.into()).is_none());
    }

    #[test]
    fn entries_keep_the_body_out_and_previews_bounded() {
        let entry = entry_from(&serde_json::json!({
            "seq": 3, "kind": "card", "title": "Alfred pidió a Ernesto",
            "summary": "Plan", "status": "running"
        }))
        .unwrap();
        assert_eq!(entry.kind, "card");
        assert_eq!(entry.body, "");
        assert_eq!(entry.status.as_deref(), Some("running"));

        let long = "a".repeat(130);
        let preview = preview(&format!("{long}\nsecond"));
        assert_eq!(preview.chars().count(), 120);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn a_created_conversation_and_section_decode() {
        let conversation = parse_chat_conversation(
            r#"{"ok":true,"conversation":{"id":"cnv_2","title":"Nuevo chat",
                "last_seq":0,"last_read_seq":0,"unread":0,"created_at":"t"}}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(conversation.id, "cnv_2");
        assert_eq!(conversation.title, "Nuevo chat");

        let section = parse_chat_section(
            r#"{"ok":true,"section":{"id":"sec_1","name":"Trabajo","order":2}}"#.to_string(),
        )
        .unwrap();
        assert_eq!(section.name, "Trabajo");
        assert_eq!(section.order, 2);
    }
}
