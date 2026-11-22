//! One rule of the policy, and the facts about an action that a rule is matched
//! against.

use regex::Regex;

use crate::model_approval::Risk;
use crate::policy::category::Category;
use crate::policy::classify::Location;

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
    pub(super) fn matches(&self, category: Category, facts: &Facts<'_>) -> bool {
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
