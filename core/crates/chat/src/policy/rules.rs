//! The rules of the host policy (level 1). The first rule that matches
//! decides, the file order is the priority, and a category in
//! `never_delegate` never reaches a rule: it goes to the user.

use regex::Regex;

use super::category::Category;
use super::classify::Location;
use crate::model_approval::Risk;
use crate::scorer::ScorerConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
}

/// Where a rule applies: inside the project, inside the operator's workspaces
/// root (any project folder under it), or anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Project,
    Workspace,
    Any,
}

impl Scope {
    fn admits(self, facts: &Facts<'_>) -> bool {
        match self {
            Self::Project => facts.location == Location::Project,
            Self::Workspace => facts.location != Location::Unknown && facts.in_workspace,
            Self::Any => true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub effect: Effect,
    pub categories: Vec<Category>,
    pub tools: Option<Vec<String>>,
    pub scope: Scope,
    pub command: Option<Regex>,
}

impl Rule {
    fn matches(&self, category: Category, facts: &Facts<'_>) -> bool {
        self.categories.contains(&category)
            && self
                .tools
                .as_ref()
                .is_none_or(|tools| tools.iter().any(|tool| tool == facts.tool))
            && self.scope.admits(facts)
            && self.command.as_ref().is_none_or(|pattern| {
                facts
                    .command
                    .is_some_and(|command| pattern.is_match(command))
            })
    }
}

/// What the policy knows about one action.
pub struct Facts<'a> {
    pub category: Option<Category>,
    pub tool: &'a str,
    pub command: Option<&'a str>,
    pub location: Location,
    /// The cwd is under the operator's `workspaces_root`.
    pub in_workspace: bool,
    pub risk: Risk,
}

/// The level a pending approval is sent to. The store names the levels an
/// approval climbs through; the rules only say where they send it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Lead,
    User,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::User => "user",
        }
    }
}

/// Why a pending approval left the policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NoRule,
    UnknownAction,
    HighRisk,
    NeverDelegate,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoRule => "no_rule",
            Self::UnknownAction => "unknown_action",
            Self::HighRisk => "high_risk",
            Self::NeverDelegate => "never_delegate",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow { rule: String },
    Deny { rule: String },
    Escalate { level: Level, reason: Reason },
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, effect: Effect, categories: &[Category], scope: Scope) -> Rule {
        Rule {
            id: id.to_string(),
            effect,
            categories: categories.to_vec(),
            tools: None,
            scope,
            command: None,
        }
    }

    fn facts(category: Option<Category>, location: Location, risk: Risk) -> Facts<'static> {
        Facts {
            category,
            tool: "bash",
            command: Some("cargo test"),
            location,
            in_workspace: false,
            risk,
        }
    }

    fn allowed(rule: &str) -> Verdict {
        Verdict::Allow {
            rule: rule.to_string(),
        }
    }

    #[test]
    fn the_workspace_scope_admits_any_project_folder_under_the_root_but_never_an_unplaced_action() {
        let mut inside = facts(Some(Category::Test), Location::Project, Risk::Low);
        inside.in_workspace = true;
        let mut unplaced = facts(Some(Category::Test), Location::Unknown, Risk::Low);
        unplaced.in_workspace = true;
        let outside = facts(Some(Category::Test), Location::Project, Risk::Low);
        let rule = rule("ws", Effect::Allow, &[Category::Test], Scope::Workspace);

        assert!(rule.matches(Category::Test, &inside));
        assert!(!rule.matches(Category::Test, &unplaced));
        assert!(!rule.matches(Category::Test, &outside));
    }

    #[test]
    fn without_a_file_only_read_test_and_build_inside_the_project_are_allowed() {
        let policy = Policy::defaults();
        let inside = Location::Project;
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Read), inside, Risk::Low)),
            allowed("read-in-project")
        );
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Test), inside, Risk::Low)),
            allowed("test-in-project")
        );
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Build), inside, Risk::Low)),
            allowed("build-in-project")
        );
        for category in [
            Category::Edit,
            Category::Command,
            Category::Network,
            Category::Vcs,
        ] {
            assert_eq!(
                policy.evaluate(&facts(Some(category), inside, Risk::Medium)),
                Verdict::Escalate {
                    level: Level::Lead,
                    reason: Reason::NoRule
                },
                "{category:?} has no default rule"
            );
        }
    }

    #[test]
    fn a_read_or_test_outside_the_project_is_not_allowed() {
        let policy = Policy::defaults();
        for category in [Category::Read, Category::Test] {
            assert_eq!(
                policy.evaluate(&facts(Some(category), Location::Unknown, Risk::Low)),
                Verdict::Escalate {
                    level: Level::Lead,
                    reason: Reason::NoRule
                }
            );
        }
    }

    #[test]
    fn the_default_deny_hardens_destructive_git_only() {
        let policy = Policy::defaults();
        let mut destructive = facts(Some(Category::Vcs), Location::Project, Risk::High);
        destructive.command = Some("git reset --hard HEAD~1");
        assert_eq!(
            policy.evaluate(&destructive),
            Verdict::Deny {
                rule: "git-destructive".to_string()
            }
        );
        destructive.command = Some("git status");
        assert_eq!(
            policy.evaluate(&destructive),
            Verdict::Escalate {
                level: Level::Lead,
                reason: Reason::NoRule
            }
        );
    }

    #[test]
    fn never_delegate_goes_to_the_user_even_with_an_allow_rule_for_it() {
        let policy = Policy::new(vec![rule(
            "everything",
            Effect::Allow,
            &[Category::Publish],
            Scope::Any,
        )]);
        assert_eq!(
            policy.evaluate(&facts(
                Some(Category::Publish),
                Location::Project,
                Risk::Low
            )),
            Verdict::Escalate {
                level: Level::User,
                reason: Reason::NeverDelegate
            }
        );
    }

    #[test]
    fn a_deny_rule_hardens_any_other_category() {
        let policy = Policy::new(vec![
            rule("no-builds", Effect::Deny, &[Category::Build], Scope::Any),
            rule("builds", Effect::Allow, &[Category::Build], Scope::Any),
        ]);
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Build), Location::Project, Risk::Low)),
            Verdict::Deny {
                rule: "no-builds".to_string()
            }
        );
    }

    #[test]
    fn the_first_matching_rule_wins_in_file_order() {
        let policy = Policy::new(vec![
            rule("first", Effect::Allow, &[Category::Test], Scope::Project),
            rule("second", Effect::Deny, &[Category::Test], Scope::Any),
        ]);
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Test), Location::Project, Risk::Low)),
            allowed("first")
        );
    }

    #[test]
    fn high_risk_is_never_allowed_by_the_policy() {
        let policy = Policy::defaults();
        assert_eq!(
            policy.evaluate(&facts(Some(Category::Test), Location::Project, Risk::High)),
            Verdict::Escalate {
                level: Level::Lead,
                reason: Reason::HighRisk
            }
        );
    }

    #[test]
    fn an_unknown_category_is_never_allowed() {
        let policy = Policy::defaults();
        assert_eq!(
            policy.evaluate(&facts(None, Location::Project, Risk::Low)),
            Verdict::Escalate {
                level: Level::Lead,
                reason: Reason::UnknownAction
            }
        );
    }

    #[test]
    fn a_tool_and_command_filter_must_both_match() {
        let mut filtered = rule("cargo-only", Effect::Allow, &[Category::Test], Scope::Any);
        filtered.tools = Some(vec!["bash".to_string()]);
        filtered.command = Some(Regex::new(r"^cargo test\b").expect("pattern"));
        let policy = Policy::new(vec![filtered]);

        assert_eq!(
            policy.evaluate(&facts(Some(Category::Test), Location::Project, Risk::Low)),
            allowed("cargo-only")
        );
        let mut other_tool = facts(Some(Category::Test), Location::Project, Risk::Low);
        other_tool.tool = "python";
        assert!(matches!(
            policy.evaluate(&other_tool),
            Verdict::Escalate { .. }
        ));
        let mut other_command = facts(Some(Category::Test), Location::Project, Risk::Low);
        other_command.command = Some("cargo testing");
        assert!(matches!(
            policy.evaluate(&other_command),
            Verdict::Escalate { .. }
        ));
    }
}
