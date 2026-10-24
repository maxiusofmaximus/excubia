//! Timeline models: what the conversation shows in order. A message, a card and
//! an approval line share one sequence, so the client renders them as one
//! history. Wire-facing, like the rest of `model`.

use serde::Serialize;

use crate::model::{preview, Message};

#[derive(Clone, Debug, Serialize)]
pub struct TimelineEntry {
    pub seq: i64,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<CardLinks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decider_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decider_id: Option<String>,
    pub created_at: String,
}

/// What an approval leaves in the timeline, as the timeline table stores it.
pub struct ApprovalLine {
    pub seq: i64,
    pub approval_id: String,
    pub status: String,
    pub summary: String,
    pub decider_kind: Option<String>,
    pub decider_id: Option<String>,
    pub created_at: String,
}

/// Where a card points. The agent link reuses the detail clients already have;
/// the delegation link is the new full record. No pane detail is duplicated.
#[derive(Clone, Debug, Serialize)]
pub struct CardLinks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub delegation: String,
}

impl TimelineEntry {
    pub fn message(message: &Message) -> Self {
        Self {
            seq: message.seq,
            kind: "message",
            message_id: Some(message.id.clone()),
            author_kind: Some(message.author_kind.clone()),
            author_agent_id: message.author_agent_id.clone(),
            preview: Some(preview(&message.body_md)),
            state: Some(message.state.clone()),
            card_id: None,
            delegation_id: None,
            ref_id: None,
            title: None,
            summary: None,
            status: None,
            agent_id: message.author_agent_id.clone(),
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: None,
            decider_id: None,
            created_at: message.created_at.clone(),
        }
    }

    pub fn approval(line: ApprovalLine) -> Self {
        Self {
            seq: line.seq,
            kind: "approval",
            message_id: None,
            author_kind: None,
            author_agent_id: None,
            preview: None,
            state: Some(line.status.clone()),
            card_id: None,
            delegation_id: None,
            ref_id: Some(line.approval_id),
            title: None,
            summary: Some(line.summary),
            status: Some(line.status),
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: line.decider_kind,
            decider_id: line.decider_id,
            created_at: line.created_at,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TimelinePage {
    pub entries: Vec<TimelineEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before_seq: Option<i64>,
}
