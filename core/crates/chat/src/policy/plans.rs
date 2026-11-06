//! The `[plans]` table of `approvals.toml`: the step count above which a
//! delegation waits for an approved plan, and whether a plan that reaches
//! neither mail nor the network may be approved by rule. One bad value rejects
//! the whole file, as a bad rule does.

use serde::Deserialize;

use crate::model_plan::{reaches_out, Touch, MAX_STEPS};

/// The step count a delegation may have without a plan.
pub const DEFAULT_THRESHOLD: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanRules {
    threshold: u32,
    auto_approve_safe: bool,
}

impl Default for PlanRules {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_THRESHOLD,
            auto_approve_safe: false,
        }
    }
}

impl PlanRules {
    /// More steps than the threshold means the delegation needs its plan first.
    pub fn requires_plan(&self, steps: u32) -> bool {
        steps > self.threshold
    }

    /// Approved by rule only when the file allows it and nothing it touches
    /// reaches out: a plan with mail or network always waits for a person.
    pub fn may_auto_approve(&self, touches: &[Touch]) -> bool {
        self.auto_approve_safe && !reaches_out(touches)
    }
}

#[derive(Deserialize)]
pub struct RawPlans {
    threshold: Option<u32>,
    auto_approve_safe: Option<bool>,
}

impl RawPlans {
    pub fn into_rules(self) -> Result<PlanRules, String> {
        let threshold = self.threshold.unwrap_or(DEFAULT_THRESHOLD);
        if !(1..=MAX_STEPS as u32).contains(&threshold) {
            return Err("plans.threshold must be between 1 and 20".to_string());
        }
        Ok(PlanRules {
            threshold,
            auto_approve_safe: self.auto_approve_safe.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_steps_need_no_plan_and_four_do() {
        let rules = PlanRules::default();
        assert_eq!(rules.threshold, 3);
        assert!(!rules.requires_plan(3));
        assert!(rules.requires_plan(4));
    }

    #[test]
    fn the_threshold_is_bounded() {
        let parse = |threshold: u32| {
            RawPlans {
                threshold: Some(threshold),
                auto_approve_safe: None,
            }
            .into_rules()
        };
        assert!(parse(0).is_err());
        assert!(parse(21).is_err());
        assert_eq!(parse(20).expect("20 is the most steps").threshold, 20);
    }

    #[test]
    fn auto_approval_is_off_unless_asked_and_never_reaches_out() {
        let off = PlanRules::default();
        assert!(!off.may_auto_approve(&[Touch::Files]));

        let on = RawPlans {
            threshold: None,
            auto_approve_safe: Some(true),
        }
        .into_rules()
        .expect("rules");
        assert!(on.may_auto_approve(&[Touch::Files, Touch::Tools]));
        assert!(!on.may_auto_approve(&[Touch::Files, Touch::Mail]));
        assert!(!on.may_auto_approve(&[Touch::Network]));
    }
}
