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
    /// Additive field (`state.md`): Unix ms (UTC) of the last movement; absent
    /// when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_ms: Option<u64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(last_active_ms: Option<u64>) -> TriageRow {
        TriageRow {
            pane_id: "w1:p1".to_string(),
            workspace_id: "w1".to_string(),
            project: "p".to_string(),
            agent: "claude".to_string(),
            status: Status::Idle,
            attention: 0,
            title: None,
            cwd: "C:\\p".to_string(),
            focused: false,
            can_resume: false,
            restored: false,
            last_active_ms,
        }
    }

    #[test]
    fn last_active_is_omitted_when_unknown_and_sent_when_known() {
        let unknown = serde_json::to_value(row(None)).expect("json");
        assert!(unknown.get("last_active_ms").is_none());
        let known = serde_json::to_value(row(Some(1_793_880_000_000))).expect("json");
        assert_eq!(known["last_active_ms"], 1_793_880_000_000u64);
    }
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
