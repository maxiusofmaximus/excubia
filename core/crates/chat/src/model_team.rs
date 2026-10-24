//! The team models of the RFC: projects, seats, delegations and their cards.
//! They live apart from the conversation models because they grow with the
//! team surface and share nothing but [`crate::model::TimelineEntry`].

use serde::Serialize;

use crate::model::{CardLinks, Message, TimelineEntry};

/// The UAC state of a seat whose blocking adapter was never checked.
pub const UAC_UNCHECKED: &str = "unchecked";
/// The UAC state of a seat whose blocking adapter was verified.
pub const UAC_SAFE: &str = "safe";
/// The UAC state of a seat whose blocking adapter failed the preflight.
pub const UAC_UNSAFE: &str = "unsafe";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DelegationState {
    Queued,
    Running,
    Waiting,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

impl DelegationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "waiting" => Some(Self::Waiting),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Waiting)
    }

    pub fn is_terminal(self) -> bool {
        !self.is_active()
    }
}

/// A project seat as the team surface publishes it.
#[derive(Clone, Debug, Serialize)]
pub struct Seat {
    pub agent_id: String,
    pub name: String,
    pub role_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub status: String,
    /// Whether the blocking UAC adapter of this seat was verified:
    /// `unchecked`, `safe` or `unsafe` (CORE8).
    pub uac_state: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Delegation {
    pub id: String,
    pub conversation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub from_agent_id: String,
    pub from_name: String,
    pub to_agent_id: String,
    pub to_name: String,
    pub task: String,
    pub state: DelegationState,
    pub depth: i64,
    pub turn_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: String,
    pub conversation_id: String,
    pub delegation_id: String,
    pub seq: i64,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub attention: i64,
    pub links: CardLinks,
    pub created_at: String,
    pub updated_at: String,
}

impl Card {
    pub fn timeline_entry(&self) -> TimelineEntry {
        TimelineEntry {
            seq: self.seq,
            kind: "card",
            message_id: None,
            author_kind: None,
            author_agent_id: Some(self.agent_id.clone()),
            preview: None,
            state: Some(self.status.clone()),
            card_id: Some(self.id.clone()),
            delegation_id: Some(self.delegation_id.clone()),
            ref_id: Some(self.delegation_id.clone()),
            title: Some(self.title.clone()),
            summary: Some(self.summary.clone()),
            status: Some(self.status.clone()),
            agent_id: Some(self.agent_id.clone()),
            pane_id: self.pane_id.clone(),
            attention: Some(self.attention),
            links: Some(self.links.clone()),
            decider_kind: None,
            decider_id: None,
            created_at: self.created_at.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SynthesisInfo {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DelegationDetail {
    pub delegation: Delegation,
    pub children: Vec<Delegation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<Card>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_message: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesis: Option<SynthesisInfo>,
}

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    pub root_path: std::path::PathBuf,
}

/// The host-side project record. `root_path` is never serialized into a
/// published summary (`orchestrator-chat.md` §1.1).
#[derive(Clone, Debug)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub status: String,
    pub lead_agent_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewSeat {
    pub project_id: String,
    pub name: String,
    pub role_id: String,
}

#[derive(Clone, Debug)]
pub struct NewDelegation {
    pub conversation_id: String,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub task: String,
    /// Alfred: the run that is asking. A lead: its own delegation id.
    pub turn_key: String,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SynthesisRow {
    pub delegation_id: String,
    pub conversation_id: String,
    pub result_message_id: String,
}
