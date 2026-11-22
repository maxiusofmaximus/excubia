//! The `[schedules]` table of `approvals.toml`: the brakes on the launches of scheduled work,
//! per oficio and per day in total. A brake that stops a run records it as skipped. One bad
//! value rejects the whole file, as the other tables do.

use serde::Deserialize;

use crate::model_schedule::{
    DEFAULT_PER_DAY_PER_AGENT, DEFAULT_PER_DAY_TOTAL, DEFAULT_PER_HOUR_PER_AGENT,
};

/// The most a brake may allow: a brake above this is a typo, not a policy.
const BRAKE_MAX: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRules {
    pub per_hour_per_agent: i64,
    pub per_day_per_agent: i64,
    pub per_day_total: i64,
}

impl Default for ScheduleRules {
    fn default() -> Self {
        Self {
            per_hour_per_agent: DEFAULT_PER_HOUR_PER_AGENT,
            per_day_per_agent: DEFAULT_PER_DAY_PER_AGENT,
            per_day_total: DEFAULT_PER_DAY_TOTAL,
        }
    }
}

#[derive(Deserialize)]
pub struct RawSchedules {
    per_hour_per_agent: Option<i64>,
    per_day_per_agent: Option<i64>,
    per_day_total: Option<i64>,
}

impl RawSchedules {
    pub fn into_rules(self) -> Result<ScheduleRules, String> {
        let defaults = ScheduleRules::default();
        let rules = ScheduleRules {
            per_hour_per_agent: self
                .per_hour_per_agent
                .unwrap_or(defaults.per_hour_per_agent),
            per_day_per_agent: self.per_day_per_agent.unwrap_or(defaults.per_day_per_agent),
            per_day_total: self.per_day_total.unwrap_or(defaults.per_day_total),
        };
        for (name, value) in [
            ("per_hour_per_agent", rules.per_hour_per_agent),
            ("per_day_per_agent", rules.per_day_per_agent),
            ("per_day_total", rules.per_day_total),
        ] {
            if !(1..=BRAKE_MAX).contains(&value) {
                return Err(format!(
                    "schedules.{name} must be between 1 and {BRAKE_MAX}"
                ));
            }
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_one_an_hour_four_a_day_and_ten_in_total() {
        let rules = RawSchedules {
            per_hour_per_agent: None,
            per_day_per_agent: None,
            per_day_total: None,
        }
        .into_rules()
        .expect("defaults");
        assert_eq!(rules, ScheduleRules::default());
        assert_eq!(
            (
                rules.per_hour_per_agent,
                rules.per_day_per_agent,
                rules.per_day_total
            ),
            (1, 4, 10)
        );
    }

    #[test]
    fn a_brake_outside_its_range_rejects_the_table() {
        let zero = RawSchedules {
            per_hour_per_agent: Some(0),
            per_day_per_agent: None,
            per_day_total: None,
        };
        assert!(zero.into_rules().is_err());
        let too_many = RawSchedules {
            per_hour_per_agent: None,
            per_day_per_agent: None,
            per_day_total: Some(101),
        };
        assert!(too_many.into_rules().is_err());
    }
}
