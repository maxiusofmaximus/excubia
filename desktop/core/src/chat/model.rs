//! Wire-facing chat models. Unknown fields and newer timeline kinds stay
//! readable: the shell draws a neutral line instead of failing the page.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ChatEvent {
    pub event: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl ChatEvent {
    pub fn from_frame(event: &str, value: &Value) -> Self {
        let text = |field: &str| value.get(field).and_then(Value::as_str).map(str::to_string);
        Self {
            event: event.to_string(),
            revision: value.get("revision").and_then(Value::as_u64).unwrap_or(0),
            conversation_id: text("conversation_id"),
            message_id: text("message_id"),
            seq: value.get("seq").and_then(Value::as_i64),
            change: text("change"),
            state: text("state"),
            delegation_id: text("delegation_id"),
            card_id: text("card_id"),
            pane_id: text("pane_id"),
            status: text("status"),
            summary: text("summary"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Company {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeatSummary {
    pub agent_id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub uac_state: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub root_label: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lead: Option<SeatSummary>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Unread {
    #[serde(default)]
    pub conversations: u64,
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub approvals: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChatSummary {
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub unread: u64,
    #[serde(default)]
    pub pending_approvals: u64,
    /// Approvals across every conversation that wait for this operator.
    #[serde(default)]
    pub pending_user_approvals: u64,
    #[serde(default)]
    pub active_delegations: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatBootstrap {
    pub company: Company,
    #[serde(default)]
    pub projects: Vec<ProjectSummary>,
    pub alfred: SeatSummary,
    #[serde(default)]
    pub unread: Unread,
    #[serde(default)]
    pub chat: ChatSummary,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub last_message_at: String,
    #[serde(default)]
    pub last_seq: i64,
    #[serde(default)]
    pub last_read_seq: i64,
    #[serde(default)]
    pub unread: u64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ConversationPage {
    #[serde(default)]
    pub conversations: Vec<Conversation>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationDetail {
    pub conversation: Conversation,
    #[serde(default)]
    pub participants: Vec<SeatSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub seq: i64,
    #[serde(default)]
    pub author_kind: String,
    #[serde(default)]
    pub author_agent_id: Option<String>,
    #[serde(default)]
    pub body_md: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessagePage {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub next_before_seq: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CardLinks {
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub delegation: Option<String>,
}

/// One timeline row. `kind` is kept as a string: a newer host may add kinds
/// and the shell must draw a neutral line instead of failing the page.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimelineEntry {
    pub seq: i64,
    pub kind: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub author_kind: Option<String>,
    #[serde(default)]
    pub author_agent_id: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub card_id: Option<String>,
    #[serde(default)]
    pub delegation_id: Option<String>,
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub attention: Option<i64>,
    #[serde(default)]
    pub links: Option<CardLinks>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimelinePage {
    #[serde(default)]
    pub entries: Vec<TimelineEntry>,
    #[serde(default)]
    pub next_before_seq: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Turn {
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub agent_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendAccepted {
    pub message: Message,
    pub turn: Turn,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadAccepted {
    #[serde(default)]
    pub last_read_seq: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Section {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub order: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SectionPage {
    #[serde(default)]
    pub sections: Vec<Section>,
}
