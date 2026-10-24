//! Wire-facing chat models. The RFC (`docs/rfc/orchestrator-chat.md`) defines
//! the fields; internal columns (company id, preview source) stay out of these
//! types unless a route needs them.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Company {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeatSummary {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    /// Whether the blocking UAC adapter of this seat was verified:
    /// `unchecked`, `safe` or `unsafe` (CORE8).
    pub uac_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub root_label: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead: Option<SeatSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Unread {
    pub conversations: u64,
    pub messages: u64,
    pub approvals: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChatSummary {
    pub revision: i64,
    /// Unread agent messages across the company.
    pub unread: u64,
    pub pending_approvals: u64,
    /// The pending approvals the operator has to decide (CORE9-D).
    pub pending_user_approvals: u64,
    pub active_delegations: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Bootstrap {
    pub company: Company,
    pub projects: Vec<ProjectSummary>,
    pub alfred: SeatSummary,
    pub unread: Unread,
    pub chat: ChatSummary,
}

#[derive(Clone, Debug, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    pub archived: bool,
    pub last_message_at: String,
    pub last_seq: i64,
    pub last_read_seq: i64,
    pub unread: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl Conversation {
    /// A brand-new conversation has no messages.
    pub fn empty(
        id: String,
        title: String,
        project_id: Option<String>,
        created_at: String,
    ) -> Self {
        Self {
            id,
            title,
            project_id,
            pinned: false,
            section_id: None,
            archived: false,
            last_message_at: created_at.clone(),
            last_seq: 0,
            last_read_seq: 0,
            unread: 0,
            created_at: created_at.clone(),
            updated_at: created_at,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ConversationPage {
    pub conversations: Vec<Conversation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConversationDetail {
    pub conversation: Conversation,
    pub participants: Vec<SeatSummary>,
}

#[derive(Clone, Debug)]
pub struct NewConversation {
    pub title: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum SectionChange {
    Keep,
    Clear,
    Set(String),
}

#[derive(Clone, Debug)]
pub struct ConversationUpdate {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub section: SectionChange,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct ConversationFilter {
    pub q: Option<String>,
    pub section_id: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub seq: i64,
    pub author_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_agent_id: Option<String>,
    pub body_md: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Queued,
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            // CORE6A called a replied turn `exited`; keep old rows readable.
            "exited" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// A turn as the seat sees it. Only `id`, `state` and `agent_id` go on the
/// wire; the rest is the payload the future MCP adapter will need.
#[derive(Clone, Debug, Serialize)]
pub struct Turn {
    pub id: String,
    pub state: RunState,
    pub agent_id: String,
    #[serde(skip)]
    pub conversation_id: String,
    #[serde(skip)]
    pub message_id: String,
    #[serde(skip)]
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct AcceptedMessage {
    pub message: Message,
    pub turn: Turn,
    /// True when `client_id` matched an earlier message: the caller must not
    /// deliver the turn a second time.
    pub duplicate: bool,
}

pub use crate::model_timeline::{ApprovalLine, CardLinks, TimelineEntry, TimelinePage};

#[derive(Clone, Debug, Serialize)]
pub struct MessagePage {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before_seq: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub order: i64,
}

/// The bounded conversation context the seat sends to a CLI: at most the last
/// 20 messages and 24 000 characters (`orchestrator-chat.md` §8).
#[derive(Clone, Debug)]
pub struct ContextPage {
    pub messages: Vec<Message>,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct ChatFrame {
    pub id: i64,
    pub event: String,
    pub body: serde_json::Value,
}

/// One line of a message, bounded: SSE and the timeline never carry the full
/// body (`orchestrator-chat.md` §3.2).
pub fn preview(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let line = text.lines().next().unwrap_or_default();
    let mut chars = line.chars();
    let mut preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        // The ellipsis is part of the bound, not extra.
        preview.pop();
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_one_bounded_line() {
        assert_eq!(preview("hola\nmundo"), "hola");
        assert_eq!(preview(&"a".repeat(130)).chars().count(), 120);
        assert!(preview(&"a".repeat(130)).ends_with('…'));
        assert_eq!(preview(&"a".repeat(120)), "a".repeat(120));
    }

    #[test]
    fn run_states_round_trip() {
        for state in [
            RunState::Queued,
            RunState::Running,
            RunState::Completed,
            RunState::Failed,
            RunState::Interrupted,
        ] {
            assert_eq!(RunState::from_db(state.as_str()), Some(state));
        }
        assert_eq!(
            RunState::from_db("exited"),
            Some(RunState::Completed),
            "CORE6A rows stay readable"
        );
        assert_eq!(RunState::from_db("nope"), None);
    }
}
