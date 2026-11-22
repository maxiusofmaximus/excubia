//! Wire-facing chat models and lenient decoders (`docs/contracts/api.md`,
//! `/v1/chat`). Unknown fields are ignored; unknown timeline kinds are kept as
//! a neutral entry so an older phone never breaks on a newer card.

use serde_json::Value;

use super::attach_model::{attachments_from, ChatAttachment};

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
    /// Turns of this conversation that are `queued` or `running`; 0 when the host
    /// omits it.
    pub active_runs: u32,
    /// The step Alfred set on the open turn, as one plain line (`progress_text`).
    pub progress: Option<String>,
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
    /// Alfred's agent id, the `seat_id` of the approvals he asks for; None when
    /// the host sends none.
    pub alfred_agent_id: Option<String>,
    pub unread_conversations: u64,
    pub unread_messages: u64,
    pub pending_approvals: u64,
    pub revision: i64,
    pub projects: Vec<super::model_team::ChatProject>,
}

/// The numbers of the history chips. `all` and `attention` and `working` skip
/// archived chats, as the default list does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCounts {
    pub all: u64,
    pub attention: u64,
    pub working: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSection {
    pub id: String,
    pub name: String,
    pub order: i64,
}

/// The «Conecta tu correo» card of a mail sign-in (contract `mail.md`, «Tarjeta de
/// inicio de sesión»). It carries the code and the page only; the host removes
/// the card before it publishes the final state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailCard {
    pub account: String,
    pub user_code: String,
    pub verification_uri: String,
    /// ISO-8601 UTC with milliseconds; empty when the host sent none.
    pub expires_at: String,
}

/// The «Revisar borrador» card of a draft Alfred wrote (`kind: mail_draft`). The
/// host sends one per `draft_id`; a newer one for the same draft replaces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailDraftCard {
    pub account: String,
    pub draft_id: String,
    pub to: Vec<String>,
    pub subject: String,
}

/// The message a reply answers, resolved by the host when the page was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatQuote {
    pub kind: String,
    pub id: String,
    pub author_kind: String,
    pub preview: String,
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
    /// Set only on a user message that has a turn: `queued`, `running`,
    /// `completed`, `failed` or `interrupted`.
    pub turn_state: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub approval_id: Option<String>,
    pub level: Option<String>,
    pub decider_kind: Option<String>,
    pub decider_id: Option<String>,
    /// Set only on a sign-in card: a card without `user_code` is a delegation.
    pub mail: Option<MailCard>,
    /// Set only on a `mail_draft` row.
    pub mail_draft: Option<MailDraftCard>,
    /// Agent messages only: up to four one-line replies Alfred offers.
    pub suggestions: Vec<String>,
    /// Set only on a user message that answers another message.
    pub quote: Option<ChatQuote>,
    /// Set only when the timeline row has an `attachments` key; the raw message
    /// sync never carries it, so a body refresh keeps the files already shown.
    pub attachments: Option<Vec<ChatAttachment>>,
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
    pub turn_state: Option<String>,
    /// The line of a `chat.progress` frame; `None` when it closes the run.
    pub progress: Option<String>,
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
pub fn parse_chat_counts(json: String) -> Option<ChatCounts> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(ChatCounts {
        all: count(Some(&value), "all"),
        attention: count(Some(&value), "attention"),
        working: count(Some(&value), "working"),
    })
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

/// `stopped` is false when no turn was open: a second stop is not an error.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_chat_stop(json: String) -> Option<bool> {
    let value: Value = serde_json::from_str(&json).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    value.get("stopped")?.as_bool()
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
        alfred_agent_id: text(alfred, "agent_id"),
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
    let progress = (event == "chat.progress")
        .then(|| progress_text(value.get("text")))
        .flatten();
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
        turn_state: text(value, "turn_state"),
        progress,
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
        active_runs: value
            .get("active_runs")
            .and_then(Value::as_u64)
            .and_then(|runs| u32::try_from(runs).ok())
            .unwrap_or(0),
        progress: progress_text(value.get("progress")),
        created_at,
        updated_at: text(value, "updated_at").unwrap_or_default(),
    })
}

/// The step Alfred writes, as one plain line: control characters that are not
/// spaces are dropped, runs of spaces collapse, at most 120 characters. Empty is
/// no step. Alfred writes the text, so it is never markup.
pub(crate) fn progress_text(value: Option<&Value>) -> Option<String> {
    let raw = value?.as_str()?;
    let kept: String = raw
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    let line: String = kept
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    (!line.is_empty()).then_some(line)
}

pub(crate) fn entry_from(value: &Value) -> Option<ChatEntry> {
    let seq = value.get("seq").and_then(Value::as_i64)?;
    if seq <= 0 {
        return None;
    }
    let links = value.get("links");
    let kind = text(value, "kind").unwrap_or_else(|| "unknown".to_string());
    // The history names an approval line's approval in `ref_id`; a live frame says `approval_id`.
    let approval_id = text(value, "approval_id").or_else(|| {
        (kind == "approval")
            .then(|| text(value, "ref_id"))
            .flatten()
    });
    Some(ChatEntry {
        seq,
        kind,
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
        turn_state: text(value, "turn_state"),
        title: text(value, "title"),
        summary: text(value, "summary"),
        status: text(value, "status"),
        approval_id,
        level: text(value, "level"),
        decider_kind: text(value, "decider_kind"),
        decider_id: text(value, "decider_id"),
        mail: mail_card_from(value),
        mail_draft: mail_draft_card_from(value),
        suggestions: suggestions_from(value),
        quote: quote_from(value),
        attachments: attachments_from(value),
        created_at: text(value, "created_at").unwrap_or_default(),
        local: false,
    })
}

/// Up to four replies of 1…60 characters without control characters. Anything
/// else is dropped, so a malformed row never breaks the timeline.
fn suggestions_from(value: &Value) -> Vec<String> {
    value
        .get("suggestions")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|text| {
                    (1..=60).contains(&text.chars().count()) && !text.chars().any(char::is_control)
                })
                .take(4)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn quote_from(value: &Value) -> Option<ChatQuote> {
    let quote = value.get("quote")?;
    Some(ChatQuote {
        kind: text(quote, "kind")?,
        id: text(quote, "id")?,
        author_kind: text(quote, "author_kind").unwrap_or_default(),
        preview: text(quote, "preview").unwrap_or_default(),
    })
}

fn mail_card_from(value: &Value) -> Option<MailCard> {
    Some(MailCard {
        account: text(value, "account")?,
        user_code: text(value, "user_code")?,
        verification_uri: text(value, "verification_uri")?,
        expires_at: text(value, "expires_at").unwrap_or_default(),
    })
}

fn mail_draft_card_from(value: &Value) -> Option<MailDraftCard> {
    if text(value, "kind")? != "mail_draft" {
        return None;
    }
    Some(MailDraftCard {
        account: text(value, "account")?,
        draft_id: text(value, "draft_id")?,
        to: value
            .get("to")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        subject: text(value, "subject").unwrap_or_default(),
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
        turn_state: text(value, "turn_state"),
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
    fn an_approval_line_of_the_history_carries_its_approval_in_ref_id() {
        let line = entry_from(&serde_json::json!({
            "seq": 7, "kind": "approval", "ref_id": "apr_1", "status": "approved",
            "summary": "Operador aprobó: mover", "created_at": "a"
        }))
        .unwrap();
        assert_eq!(line.approval_id.as_deref(), Some("apr_1"));
        let delegation = entry_from(&serde_json::json!({
            "seq": 8, "kind": "delegation", "ref_id": "dlg_1", "created_at": "a"
        }))
        .unwrap();
        assert_eq!(delegation.approval_id, None);
    }

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
    fn a_progress_line_is_one_plain_line_or_absent() {
        let line = |raw: &str| progress_text(Some(&Value::from(raw)));
        assert_eq!(
            line("  Revisando\n los\u{7} <b>borradores</b>  ").as_deref(),
            Some("Revisando los <b>borradores</b>")
        );
        assert_eq!(line("   \n\t "), None);
        assert_eq!(progress_text(Some(&Value::Null)), None);
        assert_eq!(progress_text(None), None);
        assert_eq!(line(&"x".repeat(200)).unwrap().chars().count(), 120);
    }

    #[test]
    fn active_runs_counts_the_open_turns_and_is_zero_when_absent() {
        let with = parse_chat_conversation(
            r#"{"ok":true,"conversation":{"id":"cnv_1","active_runs":2,"created_at":"a"}}"#
                .to_string(),
        )
        .unwrap();
        let without = parse_chat_conversation(
            r#"{"ok":true,"conversation":{"id":"cnv_1","created_at":"a"}}"#.to_string(),
        )
        .unwrap();
        assert_eq!(with.active_runs, 2);
        assert_eq!(without.active_runs, 0);
    }

    #[test]
    fn counts_decode_the_three_chips_and_need_ok() {
        let counts = parse_chat_counts(
            r#"{"ok":true,"all":5,"attention":2,"working":1,"archived":9}"#.to_string(),
        )
        .expect("counts");
        assert_eq!((counts.all, counts.attention, counts.working), (5, 2, 1));
        assert_eq!(parse_chat_counts(r#"{"ok":false}"#.to_string()), None);
    }

    #[test]
    fn a_stop_answer_says_whether_a_turn_was_open() {
        let open = parse_chat_stop(r#"{"ok":true,"stopped":true,"runs":1}"#.to_string());
        let none = parse_chat_stop(r#"{"ok":true,"stopped":false,"runs":0}"#.to_string());
        assert_eq!(open, Some(true));
        assert_eq!(none, Some(false));
        assert_eq!(parse_chat_stop(r#"{"ok":false}"#.to_string()), None);
    }

    #[test]
    fn a_turn_state_comes_from_the_message_or_a_turn_frame_only() {
        let entry = entry_from(&serde_json::json!({
            "seq": 4, "kind": "message", "message_id": "msg_4",
            "author_kind": "user", "turn_state": "queued"
        }))
        .unwrap();
        assert_eq!(entry.turn_state.as_deref(), Some("queued"));
        assert_eq!(entry.state, None);

        let frame = parse_chat_event(
            r#"{"event":"chat.message","revision":60,"conversation_id":"cnv_1",
                "message_id":"msg_4","seq":4,"change":"updated","turn_state":"running"}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(frame.turn_state.as_deref(), Some("running"));
        assert_eq!(frame.state, None);
    }

    #[test]
    fn suggestions_and_quote_decode_when_present_and_default_when_absent() {
        let agent = entry_from(&serde_json::json!({
            "seq": 5, "kind": "message", "message_id": "msg_5", "author_kind": "agent",
            "suggestions": ["Sí, créalo", "", "Cámbialo a Beta", "x\u{7}", "a", "b", "c"]
        }))
        .unwrap();
        assert_eq!(
            agent.suggestions,
            ["Sí, créalo", "Cámbialo a Beta", "a", "b"]
        );

        let reply = entry_from(&serde_json::json!({
            "seq": 6, "kind": "message", "message_id": "msg_6", "author_kind": "user",
            "quote": {"kind": "message", "id": "msg_5", "author_kind": "agent", "preview": "¿Qué nombre?"}
        }))
        .unwrap();
        let quote = reply.quote.unwrap();
        assert_eq!(quote.id, "msg_5");
        assert_eq!(quote.author_kind, "agent");
        assert_eq!(quote.preview, "¿Qué nombre?");

        // An old host sends neither field: no chips, no quote, no error.
        let old = entry_from(&serde_json::json!({
            "seq": 7, "kind": "message", "message_id": "msg_7", "author_kind": "agent"
        }))
        .unwrap();
        assert!(old.suggestions.is_empty());
        assert_eq!(old.quote, None);
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
    fn bootstrap_carries_alfred_agent_id_only_when_the_host_sends_it() {
        let with = parse_chat_bootstrap(
            r#"{"ok":true,"alfred":{"name":"Alfred","status":"working","agent_id":"agt_alfred"}}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(with.alfred_agent_id.as_deref(), Some("agt_alfred"));

        let without = parse_chat_bootstrap(
            r#"{"ok":true,"alfred":{"name":"Alfred","status":"working"}}"#.to_string(),
        )
        .unwrap();
        assert_eq!(without.alfred_agent_id, None);
        assert_eq!(without.alfred_name, "Alfred");
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

    #[test]
    fn a_sign_in_card_carries_the_code_and_the_page() {
        let entry = entry_from(&serde_json::json!({
            "seq": 4, "kind": "card", "title": "Conecta tu correo", "state": "pending",
            "account": "trabajo", "user_code": "ABCD-1234",
            "verification_uri": "https://microsoft.com/devicelogin",
            "expires_at": "2026-10-25T10:15:00.000Z", "created_at": "2026-10-25T10:00:00.000Z",
            "device_code": "dc_secret", "access_token": "tok_secret", "refresh_token": "rt_secret",
            "campo_nuevo": {"anidado": true}
        }))
        .unwrap();
        let card = entry.mail.clone().expect("sign-in card");
        assert_eq!(card.account, "trabajo");
        assert_eq!(card.user_code, "ABCD-1234");
        assert_eq!(card.verification_uri, "https://microsoft.com/devicelogin");
        assert_eq!(card.expires_at, "2026-10-25T10:15:00.000Z");
        let debug = format!("{entry:?}");
        assert!(!debug.contains("dc_secret"));
        assert!(!debug.contains("tok_secret"));
        assert!(!debug.contains("rt_secret"));
    }

    #[test]
    fn a_sign_in_card_without_its_expiry_keeps_an_empty_one() {
        let card = entry_from(&serde_json::json!({
            "seq": 4, "kind": "card", "account": "trabajo", "user_code": "ABCD-1234",
            "verification_uri": "https://microsoft.com/devicelogin"
        }))
        .and_then(|entry| entry.mail)
        .expect("sign-in card");
        assert_eq!(card.expires_at, "");
    }

    #[test]
    fn a_card_without_a_code_is_a_delegation_not_a_sign_in() {
        let entry = entry_from(&serde_json::json!({
            "seq": 3, "kind": "card", "title": "Alfred pidió a Ernesto", "account": "trabajo"
        }))
        .unwrap();
        assert!(entry.mail.is_none());
        assert_eq!(entry.title.as_deref(), Some("Alfred pidió a Ernesto"));
    }

    #[test]
    fn a_draft_card_carries_the_draft_and_no_sign_in() {
        let entry = entry_from(&serde_json::json!({
            "seq": 9, "kind": "mail_draft", "account": "trabajo", "draft_id": "dft_1",
            "to": ["bea@ejemplo.org", 7], "subject": "Presupuesto", "preview": "Hola Bea",
            "created_at": "2026-10-25T10:00:00.000Z"
        }))
        .unwrap();
        let card = entry.mail_draft.clone().expect("draft card");
        assert_eq!(card.draft_id, "dft_1");
        assert_eq!(card.account, "trabajo");
        assert_eq!(card.to, vec!["bea@ejemplo.org".to_string()]);
        assert_eq!(card.subject, "Presupuesto");
        assert!(entry.mail.is_none());
        assert!(entry_from(&serde_json::json!({
            "seq": 10, "kind": "card", "account": "trabajo", "draft_id": "dft_1"
        }))
        .unwrap()
        .mail_draft
        .is_none());
    }
}
