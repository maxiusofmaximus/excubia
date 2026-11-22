//! Screen-state detection from bundled TOML manifests.
//!
//! Adapted from herdr (Apache-2.0), `src/detect/manifest.rs`
//! (`compiled_gate_matches`, `region`); this prototype keeps the gate
//! semantics and a subset of the regions. See NOTICE.
//!
//! A rule matches against a region of the screen tail (or the OSC title)
//! and yields a state. Rules are evaluated in priority order, highest first;
//! the first match wins. A rule marked `skip_state_update` reports "no
//! update" (the caller keeps its previous state).

mod gate;
mod region;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use gate::{compile_gate, gate_matches, Gate, GateFile};
use region::{parse_region, region_text, Region};

/// The states a manifest can report. `done` is not a screen state: it is
/// derived by the host from `idle` + "not seen yet" (herdr's own rule).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedState {
    Idle,
    Working,
    Blocked,
    Unknown,
}

impl DetectedState {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "blocked" => Some(Self::Blocked),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// What a successful rule evaluation produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub state: DetectedState,
    pub rule_id: String,
}

/// A compiled manifest for one agent.
#[derive(Debug)]
pub struct Detector {
    pub id: String,
    rules: Vec<CompiledRule>,
}

#[derive(Debug)]
struct CompiledRule {
    id: String,
    state: DetectedState,
    priority: i64,
    region: Region,
    gate: Gate,
    skip_state_update: bool,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    id: String,
    #[serde(default)]
    rules: Vec<RuleFile>,
}

#[derive(Debug, Deserialize)]
struct RuleFile {
    id: String,
    state: String,
    priority: i64,
    region: String,
    #[serde(flatten)]
    gate: GateFile,
    #[serde(default)]
    skip_state_update: bool,
}

impl Detector {
    /// Parse and compile a manifest. Bundled manifests are validated by a
    /// unit test, so this returning `Err` is a development error, not a
    /// runtime one.
    pub fn from_toml(text: &str) -> Result<Self> {
        let manifest: ManifestFile = toml::from_str(text).context("parse manifest")?;
        if manifest.rules.is_empty() {
            bail!("manifest {} has no rules", manifest.id);
        }
        let mut rules = Vec::with_capacity(manifest.rules.len());
        for rule in manifest.rules {
            let state = DetectedState::from_label(&rule.state)
                .with_context(|| format!("rule {} has unknown state {:?}", rule.id, rule.state))?;
            rules.push(CompiledRule {
                region: parse_region(&rule.region).with_context(|| {
                    format!("rule {} has unsupported region {:?}", rule.id, rule.region)
                })?,
                gate: compile_gate(&rule.gate)
                    .with_context(|| format!("rule {} does not compile", rule.id))?,
                id: rule.id,
                state,
                priority: rule.priority,
                skip_state_update: rule.skip_state_update,
            });
        }
        // Highest priority first; stable so equal priorities keep file order.
        rules.sort_by_key(|rule| -rule.priority);
        Ok(Self {
            id: manifest.id,
            rules,
        })
    }

    /// The manifest bundled for an agent id (`claude`, `codex`, `opencode`,
    /// `fake`).
    pub fn bundled(id: &str) -> Option<Self> {
        let text = match id {
            "claude" => include_str!("../../manifests/claude.toml"),
            "codex" => include_str!("../../manifests/codex.toml"),
            "opencode" => include_str!("../../manifests/opencode.toml"),
            "fake" => include_str!("../../manifests/fake.toml"),
            _ => return None,
        };
        Some(Self::from_toml(text).unwrap_or_else(|err| panic!("bundled manifest {id}: {err:#}")))
    }

    /// Evaluate the rules against a screen tail and the last OSC title.
    pub fn detect(&self, screen_tail: &str, osc_title: &str) -> Option<Detection> {
        for rule in &self.rules {
            let text = region_text(&rule.region, screen_tail, osc_title);
            if gate_matches(&rule.gate, &text) {
                if rule.skip_state_update {
                    return None;
                }
                return Some(Detection {
                    state: rule.state,
                    rule_id: rule.id.clone(),
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
