//! The wire shapes: the ranked state object and its rows.

use std::collections::HashMap;

use serde::Serialize;

use crate::status::Status;

/// One row of the reduced state (contract `state.md`, TriageRow).
#[derive(Clone, Debug, Serialize)]
pub struct TriageRow {
    pub pane_id: String,
    pub workspace_id: String,
    pub project: String,
    pub agent: String,
    pub status: Status,
    pub attention: u8,
    pub title: Option<String>,
    pub cwd: String,
    pub focused: bool,
    pub can_resume: bool,
    /// Additive field (`state.md`): only present on panes restored from a
    /// previous host run.
    #[serde(skip_serializing_if = "is_false")]
    pub restored: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The full state object returned by `GET /v1/state` and by the opening
/// frame of `GET /v1/events`. Exactly the contract's fields: `revision`,
/// `herdr_version` (kept for wire compatibility; the value is this core's
/// version), `focused`, `counts`, `agents`.
#[derive(Clone, Debug, Serialize)]
pub struct StatePayload {
    pub revision: u64,
    pub herdr_version: String,
    pub focused: Option<serde_json::Value>,
    pub counts: HashMap<String, u32>,
    pub agents: Vec<TriageRow>,
}

/// Contract ranking, ascending: `(-attention, focused ? 0 : 1, project.lower,
/// pane_id)`.
pub fn rank(rows: &mut [TriageRow]) {
    rows.sort_by(|a, b| {
        b.attention
            .cmp(&a.attention)
            .then_with(|| a.focused.cmp(&b.focused))
            .then_with(|| a.project.to_lowercase().cmp(&b.project.to_lowercase()))
            .then_with(|| a.pane_id.cmp(&b.pane_id))
    });
}

/// Tally per status, containing only statuses that are present.
pub fn counts(rows: &[TriageRow]) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for row in rows {
        *counts.entry(row.status.as_str().to_string()).or_insert(0) += 1;
    }
    counts
}
