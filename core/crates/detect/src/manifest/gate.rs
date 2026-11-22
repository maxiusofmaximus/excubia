//! Rule gates: the matching semantics of a manifest rule, kept from herdr
//! (Apache-2.0) `src/detect/manifest.rs` (`compiled_gate_matches`).

use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug)]
pub(super) struct Gate {
    contains: Vec<String>,
    regex: Vec<Regex>,
    line_regex: Vec<Regex>,
    all: Vec<Gate>,
    any: Vec<Gate>,
    not: Vec<Gate>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct GateFile {
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    regex: Vec<String>,
    #[serde(default)]
    line_regex: Vec<String>,
    #[serde(default)]
    all: Vec<GateFile>,
    #[serde(default)]
    any: Vec<GateFile>,
    #[serde(default, rename = "not")]
    not_gate: Vec<GateFile>,
}

pub(super) fn compile_gate(file: &GateFile) -> Result<Gate> {
    Ok(Gate {
        contains: file
            .contains
            .iter()
            .map(|needle| needle.to_lowercase())
            .collect(),
        regex: compile_all(&file.regex)?,
        line_regex: compile_all(&file.line_regex)?,
        all: file.all.iter().map(compile_gate).collect::<Result<_>>()?,
        any: file.any.iter().map(compile_gate).collect::<Result<_>>()?,
        not: file
            .not_gate
            .iter()
            .map(compile_gate)
            .collect::<Result<_>>()?,
    })
}

fn compile_all(patterns: &[String]) -> Result<Vec<Regex>> {
    patterns
        .iter()
        .map(|pattern| Regex::new(pattern).with_context(|| format!("regex {pattern:?}")))
        .collect()
}

/// The gate semantics herdr applies, in the same order.
pub(super) fn gate_matches(gate: &Gate, text: &str) -> bool {
    let lower_text = text.to_lowercase();
    if !gate
        .contains
        .iter()
        .all(|needle| lower_text.contains(needle))
    {
        return false;
    }
    if !gate.regex.iter().all(|regex| regex.is_match(text)) {
        return false;
    }
    if !gate
        .line_regex
        .iter()
        .all(|regex| text.lines().any(|line| regex.is_match(line)))
    {
        return false;
    }
    if !gate.all.iter().all(|nested| gate_matches(nested, text)) {
        return false;
    }
    if !gate.any.is_empty() && !gate.any.iter().any(|nested| gate_matches(nested, text)) {
        return false;
    }
    if gate.not.iter().any(|nested| gate_matches(nested, text)) {
        return false;
    }
    true
}
