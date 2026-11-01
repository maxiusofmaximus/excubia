//! The policy: its defaults, and the evaluation that turns the facts of an
//! action into a verdict.

use regex::Regex;

use super::{Effect, Facts, Level, Reason, Rule, Scope, Verdict};
use crate::model_approval::Risk;
use crate::policy::category::Category;
use crate::scorer::ScorerConfig;

#[derive(Clone, Debug, Default)]
pub struct Policy {
    rules: Vec<Rule>,
    scorer: ScorerConfig,
}

impl Policy {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            scorer: ScorerConfig::default(),
        }
    }

    pub fn with_scorer(self, scorer: ScorerConfig) -> Self {
        Self { scorer, ..self }
    }

    pub fn scorer(&self) -> &ScorerConfig {
        &self.scorer
    }

    /// The defaults of the RFC (§2), used while there is no `approvals.toml`.
    pub fn defaults() -> Self {
        Self::new(vec![
            Rule {
                id: "read-in-project".to_string(),
                effect: Effect::Allow,
                categories: vec![Category::Read],
                tools: None,
                scope: Scope::Project,
                command: None,
            },
            Rule {
                id: "test-in-project".to_string(),
                effect: Effect::Allow,
                categories: vec![Category::Test],
                tools: None,
                scope: Scope::Project,
                command: None,
            },
            Rule {
                id: "build-in-project".to_string(),
                effect: Effect::Allow,
                categories: vec![Category::Build],
                tools: None,
                scope: Scope::Project,
                command: None,
            },
            Rule {
                id: "git-destructive".to_string(),
                effect: Effect::Deny,
                categories: vec![Category::Vcs],
                tools: None,
                scope: Scope::Any,
                command: Some(
                    Regex::new(r"^git (reset --hard|clean -fd|checkout -- \*)")
                        .expect("the builtin pattern is valid"),
                ),
            },
        ])
    }

    pub fn evaluate(&self, facts: &Facts<'_>) -> Verdict {
        let Some(category) = facts.category else {
            return Verdict::Escalate {
                level: Level::Lead,
                reason: Reason::UnknownAction,
            };
        };
        if category.never_delegate() {
            return Verdict::Escalate {
                level: Level::User,
                reason: Reason::NeverDelegate,
            };
        }
        for rule in &self.rules {
            if !rule.matches(category, facts) {
                continue;
            }
            return match rule.effect {
                Effect::Deny => Verdict::Deny {
                    rule: rule.id.clone(),
                },
                Effect::Allow if facts.risk == Risk::High => Verdict::Escalate {
                    level: Level::Lead,
                    reason: Reason::HighRisk,
                },
                Effect::Allow => Verdict::Allow {
                    rule: rule.id.clone(),
                },
            };
        }
        Verdict::Escalate {
            level: Level::Lead,
            reason: Reason::NoRule,
        }
    }
}
