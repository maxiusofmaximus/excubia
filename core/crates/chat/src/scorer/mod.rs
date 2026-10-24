//! The risk scorer (level 1.5 of `docs/rfc/approvals-escalation.md` §2.5).
//!
//! It sits between the policy and the lead and is asked only when the rules
//! escalated an action for lack of a rule or for high risk. It can approve a
//! `low` score in a category `auto_ok_if_low` allows, and nothing else: every
//! failure, slow answer or doubt goes on to the lead.
//!
//! The backend is a small trait so more local or fallback scorers can join
//! later without touching the approval path. The protocol is OpenAI-compatible.

mod config;
mod local;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::model_approval::Risk;
use crate::policy::Category;

pub(crate) use config::{RawScorer, ScorerConfig};

/// Bumped when the question the scorer is asked changes, so an audit row
/// names the question it was answered under.
pub(crate) const PROMPT_VERSION: &str = "p1";

/// The question the scorer is asked: the canonical action, never the agent's
/// prose. Built once here, so every backend sends the same fields.
pub fn question(
    category: Category,
    tool: &str,
    cwd: &str,
    command: &str,
    paths: &[String],
) -> Value {
    json!({
        "category": category.as_str(),
        "tool": tool,
        "cwd": cwd,
        "command": command,
        "paths": paths,
    })
}

/// One scorer backend. A call answers within the budget it is given, or fails.
pub trait RiskScorer {
    /// The probability in `[0, 1]` that the action is unsafe to approve alone.
    fn score(&self, question: &Value, budget: Duration) -> Result<f64, ScorerError>;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScorerError {
    #[error("the scorer did not answer within its budget")]
    Timeout,
    #[error("the scorer is unavailable: {0}")]
    Unavailable(&'static str),
    #[error("the scorer answered something that is not a score")]
    Malformed,
}

/// What the host learned from one consultation.
#[derive(Clone, Debug, PartialEq)]
pub enum Judgement {
    Scored {
        risk_score: f64,
        risk: Risk,
        version: String,
    },
    Failed {
        version: String,
    },
}

impl Judgement {
    pub fn version(&self) -> &str {
        match self {
            Self::Scored { version, .. } | Self::Failed { version } => version,
        }
    }

    pub fn risk_score(&self) -> Option<f64> {
        match self {
            Self::Scored { risk_score, .. } => Some(*risk_score),
            Self::Failed { .. } => None,
        }
    }

    /// Whether the scorer may approve: a `low` score, in a category the file
    /// marks `auto_ok_if_low`, for an action the host does not see as high.
    pub fn approves(&self, auto_ok: bool, host_risk: Risk) -> bool {
        matches!(
            self,
            Self::Scored {
                risk: Risk::Low,
                ..
            }
        ) && auto_ok
            && host_risk != Risk::High
    }

    /// Why an approval that the scorer did not approve moves on to the lead.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Failed { .. } => "scorer_unavailable",
            Self::Scored {
                risk: Risk::Low, ..
            } => "scorer_not_auto_ok",
            Self::Scored { .. } => "scorer_risk",
        }
    }
}

/// Ask the configured scorer. `None` when the backend is `none`: nothing is
/// asked and nothing is audited. A late answer counts as no answer.
pub fn consult(config: &ScorerConfig, question: &Value) -> Option<Judgement> {
    let version = config.version()?;
    let scorer = local::build(config)?;
    let started = Instant::now();
    let answer = scorer.score(question, config.timeout);
    let in_budget = started.elapsed() <= config.timeout;
    Some(match answer {
        Ok(risk_score) if in_budget && (0.0..=1.0).contains(&risk_score) => Judgement::Scored {
            risk_score,
            risk: config.label(risk_score),
            version,
        },
        _ => Judgement::Failed { version },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(risk_score: f64, risk: Risk) -> Judgement {
        Judgement::Scored {
            risk_score,
            risk,
            version: "clef-flash@p1".to_string(),
        }
    }

    #[test]
    fn only_a_low_score_in_an_auto_ok_category_approves() {
        assert!(scored(0.05, Risk::Low).approves(true, Risk::Low));
        assert!(!scored(0.05, Risk::Low).approves(false, Risk::Low));
        assert!(!scored(0.5, Risk::Medium).approves(true, Risk::Low));
        assert!(!Judgement::Failed {
            version: "x".to_string()
        }
        .approves(true, Risk::Low));
    }

    #[test]
    fn a_high_risk_action_is_never_approved_by_the_scorer() {
        assert!(!scored(0.01, Risk::Low).approves(true, Risk::High));
    }

    #[test]
    fn each_refusal_names_its_reason() {
        assert_eq!(
            Judgement::Failed {
                version: "x".to_string()
            }
            .reason(),
            "scorer_unavailable"
        );
        assert_eq!(scored(0.05, Risk::Low).reason(), "scorer_not_auto_ok");
        assert_eq!(scored(0.9, Risk::High).reason(), "scorer_risk");
    }

    #[test]
    fn no_backend_asks_nothing() {
        let asked = question(Category::Read, "bash", ".", "ls", &[]);
        assert_eq!(consult(&ScorerConfig::default(), &asked), None);
    }
}
