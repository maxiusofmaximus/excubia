//! The host policy applied to a new approval. The rules decide at creation:
//! an allowed or denied action never waits and never spends its TTL.

use std::sync::PoisonError;

use rusqlite::{Connection, OptionalExtension};

use super::approval_level::ApprovalLevel;
use super::ChatStore;
use crate::error::ChatError;
use crate::model_approval::{ApprovalStatus, NewApproval, Risk};
use crate::model_mode::ApprovalMode;
use crate::policy::{
    assess_risk, classify, in_workspace, locate, Category, Facts, Level, Reason, Verdict,
};
use crate::scorer::{consult, question, Judgement};

/// The reason of an approval that the operator's `always_user` mode sent up.
const ALWAYS_USER: &str = "always_user";

/// The actor a scorer decision names in the audit. Its model and prompt
/// revision are the row's `scorer_version`, never the actor.
pub(super) const SCORER_ACTOR: &str = "scorer";

/// The policy's answer for one new approval, with the risk it was judged at.
#[derive(PartialEq)]
pub(super) struct Assessment {
    pub verdict: Verdict,
    pub risk: Risk,
    /// `None` when the action could not be classified.
    pub category: Option<Category>,
}

/// Where the verdict lands in the new row.
pub(super) struct Placement {
    pub risk: Risk,
    pub status: ApprovalStatus,
    /// `policy` once a rule decided, `scorer` once the scorer approved, otherwise
    /// the level that waits for it.
    pub level: ApprovalLevel,
    pub escalated_from: Option<ApprovalLevel>,
    pub reason: Option<&'static str>,
    /// The rule that decided, when the policy decided.
    pub rule: Option<String>,
    /// The deadline of the level it waits at; NULL for `user` and for decided rows.
    pub level_expires_at: Option<String>,
    /// What the scorer said, when it was asked (RFC §2.5).
    pub judgement: Option<Judgement>,
}

impl Placement {
    /// The placement under the approval mode (CORE9-D). `always_user` sends an
    /// approval the policy would have allowed, or sent to an agent level, to the
    /// operator. A denial stands, and so does a row already at `user`.
    pub(super) fn under(self, mode: ApprovalMode) -> Self {
        let waits_for_agent =
            self.status == ApprovalStatus::Pending && self.level != ApprovalLevel::User;
        let allowed = self.status == ApprovalStatus::Approved;
        if mode == ApprovalMode::Delegated || !(waits_for_agent || allowed) {
            return self;
        }
        Self {
            status: ApprovalStatus::Pending,
            level: ApprovalLevel::User,
            escalated_from: Some(self.escalated_from.unwrap_or(ApprovalLevel::Policy)),
            reason: Some(ALWAYS_USER),
            rule: None,
            level_expires_at: None,
            ..self
        }
    }

    pub(super) fn of(verdict: &Verdict, risk: Risk) -> Self {
        match verdict {
            Verdict::Allow { rule } => Self::decided(risk, ApprovalStatus::Approved, rule),
            Verdict::Deny { rule } => Self::decided(risk, ApprovalStatus::Denied, rule),
            Verdict::Escalate { level, reason } => Self {
                risk,
                status: ApprovalStatus::Pending,
                level: ApprovalLevel::from(*level),
                escalated_from: Some(ApprovalLevel::Policy),
                reason: Some(reason.as_str()),
                rule: None,
                level_expires_at: None,
                judgement: None,
            },
        }
    }

    /// The placement after the scorer judged the action. An approval says the
    /// scorer decided it; anything else climbs to the lead, whose clock starts
    /// from the scorer.
    pub(super) fn scored(self, judgement: Judgement, approves: bool) -> Self {
        if approves {
            return Self {
                status: ApprovalStatus::Approved,
                level: ApprovalLevel::Scorer,
                escalated_from: Some(ApprovalLevel::Policy),
                reason: None,
                rule: None,
                level_expires_at: None,
                judgement: Some(judgement),
                ..self
            };
        }
        Self {
            status: ApprovalStatus::Pending,
            level: ApprovalLevel::Lead,
            escalated_from: Some(ApprovalLevel::Scorer),
            reason: Some(judgement.reason()),
            rule: None,
            level_expires_at: None,
            judgement: Some(judgement),
            ..self
        }
    }

    /// Who decided the approval, when the host decided it: the rule of the
    /// policy, or the scorer. Nothing pending names a decider.
    pub(super) fn decider(&self) -> Option<(&'static str, &str)> {
        if self.status == ApprovalStatus::Pending {
            return None;
        }
        match (&self.rule, &self.judgement) {
            (Some(rule), _) => Some(("policy", rule)),
            (None, Some(_)) => Some(("scorer", SCORER_ACTOR)),
            (None, None) => None,
        }
    }

    fn decided(risk: Risk, status: ApprovalStatus, rule: &str) -> Self {
        Self {
            risk,
            status,
            level: ApprovalLevel::Policy,
            escalated_from: None,
            reason: None,
            rule: Some(rule.to_string()),
            level_expires_at: None,
            judgement: None,
        }
    }
}

impl ChatStore {
    /// The verdict of the rules for a new approval. The project comes from the
    /// seat that asks; without one the action cannot be placed, so the rules
    /// never allow it.
    pub(super) fn assess(
        &self,
        connection: &Connection,
        new: &NewApproval,
        seat_id: Option<&str>,
    ) -> Result<Assessment, ChatError> {
        let project_root = match seat_id {
            Some(seat_id) => project_root_of(connection, seat_id)?,
            None => None,
        };
        let location = locate(project_root.as_deref(), new.cwd.as_deref(), &new.paths);
        let category = classify(new.kind, &new.tool, new.payload_redacted.as_ref(), location);
        let assessed = assess_risk(category, location);
        // A caller may ask for more risk than the host sees, never for less.
        let risk = new.risk.map_or(assessed, |asked| asked.max(assessed));
        let workspace = self
            .workspaces_root
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let facts = Facts {
            category,
            tool: &new.tool,
            command: command_of(new),
            location,
            in_workspace: in_workspace(workspace.as_deref(), new.cwd.as_deref()),
            risk,
        };
        let mut policy = self.policy.lock().unwrap_or_else(PoisonError::into_inner);
        let verdict = policy.current().evaluate(&facts);
        Ok(Assessment {
            verdict,
            risk,
            category,
        })
    }

    /// The scorer's judgement of an action the rules sent to the lead for lack
    /// of a rule or for high risk (RFC §2.5), and whether it may approve. `None`
    /// when no scorer is configured, or when there is no category to judge.
    /// It reads no database: it runs with no lock held.
    pub(super) fn judge(
        &self,
        assessment: &Assessment,
        new: &NewApproval,
    ) -> Option<(Judgement, bool)> {
        let Verdict::Escalate {
            level: Level::Lead,
            reason: Reason::NoRule | Reason::HighRisk,
        } = assessment.verdict
        else {
            return None;
        };
        let category = assessment.category?;
        let config = {
            let mut policy = self.policy.lock().unwrap_or_else(PoisonError::into_inner);
            policy.current().scorer().clone()
        };
        let asked = question(
            category,
            &new.tool,
            new.cwd.as_deref().unwrap_or_default(),
            command_of(new).unwrap_or_default(),
            &new.paths,
        );
        let judgement = consult(&config, &asked)?;
        let approves = judgement.approves(config.auto_ok(category), assessment.risk);
        Some((judgement, approves))
    }
}

/// The canonical command of an action, from its redacted payload.
fn command_of(new: &NewApproval) -> Option<&str> {
    new.payload_redacted
        .as_ref()
        .and_then(|payload| payload["command"].as_str())
}

fn project_root_of(connection: &Connection, seat_id: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT project.root_path FROM agent_seat
               JOIN project ON project.id = agent_seat.project_id
              WHERE agent_seat.id = ?1",
            [seat_id],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::{ApprovalLevel, Placement};
    use crate::model_approval::{ApprovalStatus, Risk};
    use crate::policy::{Level, Reason, Verdict};

    #[test]
    fn an_allow_is_approved_by_the_policy_and_a_deny_is_denied() {
        let allowed = Placement::of(
            &Verdict::Allow {
                rule: "test-in-project".to_string(),
            },
            Risk::Low,
        );
        assert_eq!(allowed.status, ApprovalStatus::Approved);
        assert_eq!(allowed.level, ApprovalLevel::Policy);
        assert_eq!(allowed.rule.as_deref(), Some("test-in-project"));
        assert_eq!(allowed.escalated_from, None);

        let denied = Placement::of(
            &Verdict::Deny {
                rule: "git-destructive".to_string(),
            },
            Risk::High,
        );
        assert_eq!(denied.status, ApprovalStatus::Denied);
    }

    #[test]
    fn an_escalation_stays_pending_and_says_where_it_came_from() {
        let escalated = Placement::of(
            &Verdict::Escalate {
                level: Level::User,
                reason: Reason::NeverDelegate,
            },
            Risk::High,
        );
        assert_eq!(escalated.status, ApprovalStatus::Pending);
        assert_eq!(escalated.level, ApprovalLevel::User);
        assert_eq!(escalated.escalated_from, Some(ApprovalLevel::Policy));
        assert_eq!(escalated.reason, Some("never_delegate"));
        assert_eq!(escalated.rule, None);
    }
}
