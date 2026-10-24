//! `approvals.toml`: the operator's rules. It is reloaded when its mtime
//! changes. An invalid file keeps the last good policy and says why; a missing
//! file means the defaults. Nothing in the system writes this file.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use regex::Regex;
use serde::Deserialize;

use super::category::Category;
use super::rules::{Effect, Policy, Rule, Scope};
use crate::scorer::{RawScorer, ScorerConfig};

pub const POLICY_FILE: &str = "approvals.toml";

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("cannot read the file: {0}")]
    Read(#[from] std::io::Error),
    #[error("the file is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("version {0} is not supported, only version 1 is read")]
    Version(i64),
    #[error("rule `{id}`: {reason}")]
    Rule { id: String, reason: String },
    #[error("[scorer]: {0}")]
    Scorer(String),
}

#[derive(Deserialize)]
struct RawFile {
    version: i64,
    #[serde(default, rename = "rule")]
    rules: Vec<RawRule>,
    scorer: Option<RawScorer>,
}

#[derive(Deserialize)]
struct RawRule {
    id: String,
    decision: String,
    category: Vec<String>,
    tools: Option<Vec<String>>,
    cwd_under: Option<String>,
    command_matches: Option<String>,
}

impl RawRule {
    fn into_rule(self) -> Result<Rule, PolicyError> {
        let reject = |reason: String| PolicyError::Rule {
            id: self.id.clone(),
            reason,
        };
        let effect = match self.decision.as_str() {
            "allow" => Effect::Allow,
            "deny" => Effect::Deny,
            other => return Err(reject(format!("decision `{other}` is not allow or deny"))),
        };
        let scope = match self.cwd_under.as_deref() {
            None | Some("any") => Scope::Any,
            Some("project") => Scope::Project,
            Some("workspace") => Scope::Workspace,
            Some(other) => {
                return Err(reject(format!(
                    "cwd_under `{other}` is not project, workspace or any"
                )))
            }
        };
        if self.category.is_empty() {
            return Err(reject("category is empty".to_string()));
        }
        let mut categories = Vec::with_capacity(self.category.len());
        for name in &self.category {
            let category = Category::from_config(name)
                .ok_or_else(|| reject(format!("unknown category `{name}`")))?;
            if effect == Effect::Allow && category.never_delegate() {
                return Err(reject(format!("an allow rule cannot cover `{name}`")));
            }
            categories.push(category);
        }
        let command = self
            .command_matches
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|error| reject(format!("command_matches: {error}")))?;
        Ok(Rule {
            id: self.id,
            effect,
            categories,
            tools: self.tools,
            scope,
            command,
        })
    }
}

/// Parse the whole file or nothing: one bad rule rejects the file.
pub fn parse(text: &str) -> Result<Policy, PolicyError> {
    let file: RawFile = toml::from_str(text)?;
    if file.version != 1 {
        return Err(PolicyError::Version(file.version));
    }
    let rules = file
        .rules
        .into_iter()
        .map(RawRule::into_rule)
        .collect::<Result<Vec<_>, _>>()?;
    let scorer = match file.scorer {
        Some(raw) => raw.into_config().map_err(PolicyError::Scorer)?,
        None => ScorerConfig::default(),
    };
    Ok(Policy::new(rules).with_scorer(scorer))
}

/// The policy of one host, reloaded when its file changes.
pub struct PolicySource {
    path: Option<PathBuf>,
    seen: Option<SystemTime>,
    loaded: bool,
    policy: Policy,
}

impl PolicySource {
    /// No file at all: the defaults, forever.
    pub fn defaults() -> Self {
        Self {
            path: None,
            seen: None,
            loaded: false,
            policy: Policy::defaults(),
        }
    }

    /// The `approvals.toml` that sits next to the chat database.
    pub fn next_to(database: &Path) -> Self {
        let directory = database.parent().unwrap_or_else(|| Path::new("."));
        Self {
            path: Some(directory.join(POLICY_FILE)),
            ..Self::defaults()
        }
    }

    /// The policy in force now. A changed file is parsed again; a broken one
    /// keeps the policy that was in force, and the error goes to stderr once
    /// per change.
    pub fn current(&mut self) -> &Policy {
        let mtime = self.path.as_deref().and_then(modified_at);
        if !self.loaded || mtime != self.seen {
            self.policy = self.reload(mtime);
            self.seen = mtime;
            self.loaded = true;
        }
        &self.policy
    }

    fn reload(&self, mtime: Option<SystemTime>) -> Policy {
        let Some(path) = self.path.as_deref().filter(|_| mtime.is_some()) else {
            return Policy::defaults();
        };
        match std::fs::read_to_string(path)
            .map_err(PolicyError::from)
            .and_then(|text| parse(&text))
        {
            Ok(policy) => policy,
            Err(error) => {
                eprintln!("chat: {POLICY_FILE} is invalid, keeping the last good policy: {error}");
                self.policy.clone()
            }
        }
    }
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{parse, PolicySource};
    use crate::model_approval::Risk;
    use crate::policy::rules::Verdict;
    use crate::policy::{category::Category, classify::Location, rules::Facts};
    use hn_core_test_dir::TestDir;

    const VALID: &str = r#"
version = 1
mode = "delegated"

[[rule]]
id = "tests-del-proyecto"
decision = "allow"
category = ["test", "build"]
tools = ["bash"]
cwd_under = "project"
command_matches = "^(cargo (test|check|clippy)|npm test)\\b"

[[rule]]
id = "git-destructivo"
decision = "deny"
category = ["vcs"]
command_matches = "^git (reset --hard|clean -fd)"
"#;

    fn test_facts(command: &str) -> Facts<'_> {
        Facts {
            category: Some(Category::Test),
            tool: "bash",
            command: Some(command),
            location: Location::Project,
            in_workspace: false,
            risk: Risk::Low,
        }
    }

    #[test]
    fn the_rfc_example_parses_and_keeps_its_order() {
        let policy = parse(VALID).expect("valid file");
        assert_eq!(
            policy.evaluate(&test_facts("cargo test -p x")),
            Verdict::Allow {
                rule: "tests-del-proyecto".to_string()
            }
        );
        assert!(matches!(
            policy.evaluate(&test_facts("npm run lint")),
            Verdict::Escalate { .. }
        ));
    }

    #[test]
    fn a_file_replaces_the_defaults_instead_of_adding_to_them() {
        let policy = parse("version = 1\n").expect("empty file");
        let facts = Facts {
            category: Some(Category::Read),
            tool: "read",
            command: None,
            location: Location::Project,
            in_workspace: false,
            risk: Risk::Low,
        };
        assert!(matches!(policy.evaluate(&facts), Verdict::Escalate { .. }));
    }

    #[test]
    fn a_broken_file_is_rejected_whole() {
        let cases = [
            ("not = [valid", "valid TOML"),
            ("version = 2\n", "version 2"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"maybe\"\ncategory = [\"read\"]\n", "decision"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"allow\"\ncategory = [\"everything\"]\n", "unknown category"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"allow\"\ncategory = [\"publish\"]\n", "cannot cover"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"allow\"\ncategory = [\"test\"]\ncwd_under = \"outside\"\n", "cwd_under"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"allow\"\ncategory = [\"test\"]\ncommand_matches = \"(\"\n", "command_matches"),
            ("version = 1\n[[rule]]\nid = \"a\"\ndecision = \"allow\"\ncategory = []\n", "empty"),
        ];
        for (text, expected) in cases {
            let error = parse(text).expect_err(text).to_string();
            assert!(error.contains(expected), "`{expected}` not in `{error}`");
        }
    }

    #[test]
    fn a_missing_file_means_the_defaults() {
        let dir = TestDir::new("policy-missing");
        let mut source = PolicySource::next_to(&dir.join("chat.db"));
        let _ = std::fs::remove_file(dir.join(super::POLICY_FILE));
        let facts = Facts {
            category: Some(Category::Build),
            tool: "bash",
            command: None,
            location: Location::Project,
            in_workspace: false,
            risk: Risk::Low,
        };
        assert_eq!(
            source.current().evaluate(&facts),
            Verdict::Allow {
                rule: "build-in-project".to_string()
            }
        );
    }

    #[test]
    fn a_changed_file_is_reloaded_and_an_invalid_one_keeps_the_last_good_policy() {
        let dir = TestDir::new("policy-reload");
        let file = dir.join(super::POLICY_FILE);
        let mut source = PolicySource::next_to(&dir.join("chat.db"));
        let build = Facts {
            category: Some(Category::Build),
            tool: "bash",
            command: Some("cargo check"),
            location: Location::Project,
            in_workspace: false,
            risk: Risk::Low,
        };

        std::fs::write(&file, "version = 1\n").expect("write");
        assert!(matches!(
            source.current().evaluate(&build),
            Verdict::Escalate { .. }
        ));

        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&file, VALID.replace("[\"test\", \"build\"]", "[\"build\"]"))
            .expect("write");
        assert_eq!(
            source.current().evaluate(&build),
            Verdict::Allow {
                rule: "tests-del-proyecto".to_string()
            }
        );

        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&file, "version = 1\n[[rule]]\nid = \"x\"\n").expect("write");
        assert_eq!(
            source.current().evaluate(&build),
            Verdict::Allow {
                rule: "tests-del-proyecto".to_string()
            },
            "the broken file must not replace the last good policy"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
