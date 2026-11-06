//! Timeline models: what the conversation shows in order. A message, a card and
//! an approval line share one sequence, so the client renders them as one
//! history. Wire-facing, like the rest of `model`.

use serde::Serialize;

use crate::model::{preview, Message};
use crate::model_attachment::LinkedFile;

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
    /// A sign-in card: the code the user types and the page to type it on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri: Option<String>,
    /// A sign-in card: the account it signs in and when its login stops being valid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// A draft card: the draft it shows and who it goes to, with its subject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// A user message: where its turn stands. Absent when no turn was ever recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_state: Option<String>,
    /// An agent message: the quick replies it offers. Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// A user message that answers another message of the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<Quote>,
    /// A user message: the files it carries, in the order they were sent. Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<LinkedFile>,
    pub created_at: String,
}

/// The message a reply answers, as the timeline shows it. The preview is read
/// from the message when the timeline is read; no text is copied at write time.
#[derive(Clone, Debug, Serialize)]
pub struct Quote {
    pub kind: &'static str,
    pub id: String,
    pub author_kind: String,
    pub preview: String,
}

/// A sign-in card as the timeline table stores it. The card is the login's
/// face in the conversation: no device code, no token.
pub struct MailCardLine {
    pub seq: i64,
    pub card_id: String,
    pub account: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: String,
    pub created_at: String,
}

/// A reminder card as the timeline table stores it. Its title and time are read
/// from the event it names when the timeline is read.
pub struct CalendarCardLine {
    pub seq: i64,
    pub card_id: String,
    pub ref_id: String,
    pub title: String,
    pub summary: String,
    pub created_at: String,
}

/// A draft card as the timeline table stores it: the draft's recipients, subject
/// and the start of its body. The body itself never reaches the timeline.
pub struct MailDraftLine {
    pub seq: i64,
    pub card_id: String,
    pub account: String,
    pub draft_id: String,
    pub to: Vec<String>,
    pub subject: String,
    pub preview: String,
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

/// A plan line as the timeline table stores it: the status the plan reached and
/// its summary, which names the count of steps and never their titles.
pub struct PlanLine {
    pub seq: i64,
    pub plan_id: String,
    pub status: String,
    pub summary: String,
    pub decider_kind: Option<String>,
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
            user_code: None,
            verification_uri: None,
            account: None,
            expires_at: None,
            draft_id: None,
            to: None,
            subject: None,
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
            created_at: message.created_at.clone(),
        }
    }

    /// A sign-in card. It is only ever shown while its login is `pending`: the
    /// card goes away before the login's final state is published (`mail.md`).
    pub fn mail_card(line: MailCardLine) -> Self {
        Self {
            seq: line.seq,
            kind: "card",
            message_id: None,
            author_kind: None,
            author_agent_id: None,
            preview: None,
            state: Some("pending".to_owned()),
            card_id: Some(line.card_id),
            delegation_id: None,
            ref_id: None,
            title: Some("Conecta tu correo".to_owned()),
            summary: None,
            status: None,
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: None,
            decider_id: None,
            user_code: Some(line.user_code),
            verification_uri: Some(line.verification_uri),
            account: Some(line.account),
            expires_at: Some(line.expires_at),
            draft_id: None,
            to: None,
            subject: None,
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
            created_at: line.created_at,
        }
    }

    /// A reminder card of a calendar event: a card with the event as its reference.
    pub fn calendar_reminder(line: CalendarCardLine) -> Self {
        Self {
            seq: line.seq,
            kind: "card",
            message_id: None,
            author_kind: None,
            author_agent_id: None,
            preview: None,
            state: None,
            card_id: Some(line.card_id),
            delegation_id: None,
            ref_id: Some(line.ref_id),
            title: Some(line.title),
            summary: Some(line.summary),
            status: Some("reminder".to_owned()),
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: None,
            decider_id: None,
            user_code: None,
            verification_uri: None,
            account: None,
            expires_at: None,
            draft_id: None,
            to: None,
            subject: None,
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
            created_at: line.created_at,
        }
    }

    /// A draft card. It shows the draft's recipients and subject, and a preview
    /// of its body, never the body or the attachments (MAILUX-HOST §A).
    pub fn mail_draft(line: MailDraftLine) -> Self {
        Self {
            seq: line.seq,
            kind: "mail_draft",
            message_id: None,
            author_kind: None,
            author_agent_id: None,
            preview: Some(line.preview),
            state: None,
            card_id: Some(line.card_id),
            delegation_id: None,
            ref_id: None,
            title: None,
            summary: None,
            status: None,
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: None,
            decider_id: None,
            user_code: None,
            verification_uri: None,
            account: Some(line.account),
            expires_at: None,
            draft_id: Some(line.draft_id),
            to: Some(line.to),
            subject: Some(line.subject),
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
            created_at: line.created_at,
        }
    }

    /// A plan line. It reuses the approval line's fields, so a client that does
    /// not know the kind draws it as a neutral line with its summary.
    pub fn plan(line: PlanLine) -> Self {
        Self {
            seq: line.seq,
            kind: "plan",
            message_id: None,
            author_kind: None,
            author_agent_id: None,
            preview: None,
            state: Some(line.status.clone()),
            card_id: None,
            delegation_id: None,
            ref_id: Some(line.plan_id),
            title: None,
            summary: Some(line.summary),
            status: Some(line.status),
            agent_id: None,
            pane_id: None,
            attention: None,
            links: None,
            decider_kind: line.decider_kind,
            decider_id: None,
            user_code: None,
            verification_uri: None,
            account: None,
            expires_at: None,
            draft_id: None,
            to: None,
            subject: None,
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
            created_at: line.created_at,
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
            user_code: None,
            verification_uri: None,
            account: None,
            expires_at: None,
            draft_id: None,
            to: None,
            subject: None,
            turn_state: None,
            suggestions: Vec::new(),
            attachments: Vec::new(),
            quote: None,
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
