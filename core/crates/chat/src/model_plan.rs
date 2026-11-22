//! Plans (F3): the whole job an agent wants to do, read by the operator before
//! its first step. A plan is plain text, at most 20 steps, and each step says
//! what it touches. Nothing here runs an action; the store and the delegation
//! gate decide when a delegation may start.

use serde::{Deserialize, Serialize};

use crate::error::ChatError;

/// The most steps one plan may hold.
pub const MAX_STEPS: usize = 20;
/// The longest title of one step, in characters.
pub const MAX_TITLE_CHARS: usize = 120;
/// The longest note of a change request, in characters.
pub const MAX_NOTE_CHARS: usize = 300;

/// One line of a plan and what it reaches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub title: String,
    pub touches: Vec<Touch>,
}

/// What a step reaches outside the agent's own thinking. Order is the order
/// the summary lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Touch {
    Files,
    Network,
    Mail,
    Tools,
    Nothing,
}

impl Touch {
    /// The word the operator reads in the summary.
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "ficheros",
            Self::Network => "internet",
            Self::Mail => "correo",
            Self::Tools => "herramientas",
            Self::Nothing => "nada",
        }
    }
}

/// The lifecycle of a plan. `proposed` is the only open state; each other
/// one is final, and a changed plan is proposed again as a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Proposed,
    Approved,
    Rejected,
    ChangesRequested,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::ChangesRequested => "changes_requested",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "proposed" => Some(Self::Proposed),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "changes_requested" => Some(Self::ChangesRequested),
            _ => None,
        }
    }
}

/// Checks the steps of a plan. One bad step refuses the whole plan.
pub fn validate(steps: &[PlanStep]) -> Result<(), ChatError> {
    if steps.is_empty() {
        return Err(ChatError::InvalidPlan("the plan has no steps"));
    }
    if steps.len() > MAX_STEPS {
        return Err(ChatError::InvalidPlan("a plan has at most 20 steps"));
    }
    steps.iter().try_for_each(validate_step)
}

fn validate_step(step: &PlanStep) -> Result<(), ChatError> {
    let title = step.title.trim();
    if title.is_empty() {
        return Err(ChatError::InvalidPlan("a step needs a title"));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(ChatError::InvalidPlan(
            "a step title is at most 120 characters",
        ));
    }
    if !is_plain(title) {
        return Err(ChatError::InvalidPlan(
            "a step is plain text: no markup or code marks",
        ));
    }
    if step.touches.is_empty() {
        return Err(ChatError::InvalidPlan("a step says what it touches"));
    }
    Ok(())
}

/// The note of a change request: plain text, at most 300 characters.
pub fn validate_note(note: &str) -> Result<(), ChatError> {
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(ChatError::InvalidPlan("a note is at most 300 characters"));
    }
    if !is_plain(note) {
        return Err(ChatError::InvalidPlan(
            "a note is plain text: no markup or code marks",
        ));
    }
    Ok(())
}

/// Plain text has no control characters and no markup or code marks.
fn is_plain(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | '`'))
}

/// Every thing the steps reach, once each, in the order of [`Touch`].
pub fn touches_of(steps: &[PlanStep]) -> Vec<Touch> {
    let mut touches: Vec<Touch> = steps
        .iter()
        .flat_map(|step| step.touches.iter().copied())
        .collect();
    touches.sort();
    touches.dedup();
    touches
}

/// Whether a plan reaches mail or the network: such a plan is never approved by rule.
pub fn reaches_out(touches: &[Touch]) -> bool {
    touches
        .iter()
        .any(|touch| matches!(touch, Touch::Mail | Touch::Network))
}

/// The line the operator reads: how many steps, who proposes, what it touches.
pub fn summary(steps: &[PlanStep], proposer: &str) -> String {
    let count = match steps.len() {
        1 => "1 paso".to_string(),
        n => format!("{n} pasos"),
    };
    let touched: Vec<&str> = touches_of(steps)
        .iter()
        .map(|touch| touch.label())
        .collect();
    format!(
        "Plan de {count} · lo propone {proposer} · toca: {}",
        touched.join(", ")
    )
}

/// What an agent proposes: the steps, and who proposes them.
#[derive(Clone, Debug)]
pub struct NewPlan {
    pub conversation_id: String,
    pub proposer_agent_id: String,
    pub steps: Vec<PlanStep>,
}

/// The operator's answer to a proposed plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanDecision {
    Approve,
    Reject,
    RequestChanges,
}

impl PlanDecision {
    pub fn status(self) -> PlanStatus {
        match self {
            Self::Approve => PlanStatus::Approved,
            Self::Reject => PlanStatus::Rejected,
            Self::RequestChanges => PlanStatus::ChangesRequested,
        }
    }
}

/// One plan as the operator and the agents read it.
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub id: String,
    pub conversation_id: String,
    pub proposer_agent_id: String,
    pub proposer_name: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<String>,
    /// `policy` when a rule approved it, otherwise the device that decided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

#[cfg(test)]
#[path = "model_plan_tests.rs"]
mod tests;
