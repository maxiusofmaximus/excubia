use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// herdr's status vocabulary, forwarded unchanged by the gateway. Anything
/// unrecognised degrades to `Unknown`; it is never stored as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    #[default]
    Unknown,
}

impl AgentStatus {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Unknown => "unknown",
        }
    }

    pub fn attention(self) -> u8 {
        match self {
            Self::Blocked => 3,
            Self::Done => 2,
            Self::Working | Self::Unknown => 1,
            Self::Idle => 0,
        }
    }
}

impl<'de> Deserialize<'de> for AgentStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        Ok(value.as_deref().map(Self::parse).unwrap_or_default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Focused {
    pub workspace_id: Option<String>,
    pub tab_id: Option<String>,
    pub pane_id: Option<String>,
}

/// One ranked triage row, as the gateway built it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentRow {
    #[serde(deserialize_with = "string_or_empty")]
    pub pane_id: String,
    pub workspace_id: Option<String>,
    pub project: Option<String>,
    pub agent: Option<String>,
    pub status: AgentStatus,
    #[serde(deserialize_with = "u8_or_zero")]
    pub attention: u8,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub focused: bool,
    pub can_resume: bool,
    /// The pane survived a host restart but has no running agent any more.
    #[serde(default)]
    pub restored: bool,
}

/// The one state shape served by `GET /v1/state` and the opening SSE frame.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StateSnapshot {
    pub revision: u64,
    pub herdr_version: Option<String>,
    pub focused: Option<Focused>,
    pub counts: BTreeMap<String, u32>,
    pub agents: Vec<AgentRow>,
}

impl StateSnapshot {
    /// Drops rows without a pane id, normalises fallbacks, and fills counts when
    /// an older gateway omits them. Never reorders: ranking is the host's job.
    pub fn sanitize(&mut self) {
        for agent in &mut self.agents {
            agent.pane_id = agent.pane_id.trim().to_string();
            if agent.attention == 0 {
                agent.attention = agent.status.attention();
            }
            if agent
                .title
                .as_deref()
                .is_some_and(|title| title.trim().is_empty())
            {
                agent.title = None;
            }
        }
        self.agents.retain(|agent| !agent.pane_id.is_empty());

        if self.counts.is_empty() {
            for agent in &self.agents {
                *self
                    .counts
                    .entry(agent.status.as_str().to_string())
                    .or_insert(0) += 1;
            }
        }
    }
}

/// Holds the newest state. A revision older than the current one is discarded.
#[derive(Debug, Default)]
pub struct StateStore {
    current: Option<StateSnapshot>,
}

impl StateStore {
    /// Returns the stored state when the candidate became current, so the
    /// caller can hand it to a sink without cloning it again.
    pub fn apply(&mut self, mut candidate: StateSnapshot) -> Option<StateSnapshot> {
        candidate.sanitize();
        if let Some(current) = &self.current {
            if candidate.revision < current.revision {
                return None;
            }
        }
        self.current = Some(candidate);
        self.current.clone()
    }

    pub fn snapshot(&self) -> Option<StateSnapshot> {
        self.current.clone()
    }

    pub fn revision(&self) -> Option<u64> {
        self.current.as_ref().map(|state| state.revision)
    }

    /// Drops the current state. Revisions are monotonic per gateway, so a
    /// different gateway must start from an empty baseline instead of being
    /// compared against another host's revision.
    pub fn clear(&mut self) {
        self.current = None;
    }
}

fn string_or_empty<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn u8_or_zero<'de, D>(deserializer: D) -> std::result::Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<u8>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "revision": 42,
        "herdr_version": "0.9.3",
        "focused": {"workspace_id": "w1", "tab_id": "t1", "pane_id": "w1:p1"},
        "counts": {"blocked": 1, "working": 1},
        "agents": [
            {"pane_id": "w1:p1", "workspace_id": "w1", "project": "herdr-notify",
             "agent": "opencode", "status": "blocked", "attention": 3,
             "title": "Add the pairing screen", "cwd": "D:/repo", "focused": false,
             "can_resume": true},
            {"pane_id": "w1:p2", "project": "herdr-notify", "agent": "claude",
             "status": "WORKING", "attention": 1, "title": null},
            {"project": "sin pane", "status": "blocked", "attention": 3},
            {"pane_id": "w1:p3", "status": "exploded", "attention": 0,
             "title": "  "},
            {"pane_id": "w1:p4", "status": "unknown", "attention": 0,
             "title": "Anterior", "restored": true}
        ]
    }"#;

    #[test]
    fn parses_the_contract_state_and_degrades_leniently() {
        let mut state: StateSnapshot = serde_json::from_str(SAMPLE).unwrap();
        state.sanitize();
        assert_eq!(state.revision, 42);
        assert_eq!(state.herdr_version.as_deref(), Some("0.9.3"));
        assert_eq!(
            state.focused.as_ref().unwrap().pane_id.as_deref(),
            Some("w1:p1")
        );
        assert_eq!(state.agents.len(), 4, "a row without pane_id is dropped");
        assert_eq!(state.agents[0].status, AgentStatus::Blocked);
        assert_eq!(state.agents[1].status, AgentStatus::Working);
        assert_eq!(state.agents[2].status, AgentStatus::Unknown);
        assert_eq!(state.agents[2].attention, 1, "missing attention is derived");
        assert!(
            state.agents[2].title.is_none(),
            "blank titles collapse to None"
        );
        assert!(
            state.agents[3].restored,
            "the additive restored flag survives the trip"
        );
        assert_eq!(state.counts.get("blocked"), Some(&1));
    }

    #[test]
    fn fills_counts_when_an_older_gateway_omits_them() {
        let mut state: StateSnapshot = serde_json::from_str(
            r#"{"revision":1,"agents":[
                {"pane_id":"w1:p1","status":"blocked"},
                {"pane_id":"w1:p2","status":"idle"},
                {"pane_id":"w1:p3","status":"idle"}
            ]}"#,
        )
        .unwrap();
        state.sanitize();
        assert_eq!(state.counts.get("blocked"), Some(&1));
        assert_eq!(state.counts.get("idle"), Some(&2));
    }

    #[test]
    fn the_last_revision_wins_even_when_an_older_one_arrives_late() {
        let mut store = StateStore::default();
        let state = |revision: u64, pane: &str| StateSnapshot {
            revision,
            agents: vec![AgentRow {
                pane_id: pane.to_string(),
                ..AgentRow::default()
            }],
            ..StateSnapshot::default()
        };

        assert!(store.apply(state(5, "w1:p1")).is_some());
        assert!(
            store.apply(state(4, "w2:p1")).is_none(),
            "stale revision is discarded"
        );
        assert_eq!(store.snapshot().unwrap().agents[0].pane_id, "w1:p1");
        assert!(store.apply(state(6, "w3:p1")).is_some());
        assert_eq!(store.revision(), Some(6));
    }

    #[test]
    fn clearing_the_store_accepts_a_lower_revision_from_another_gateway() {
        let mut store = StateStore::default();
        store.apply(StateSnapshot {
            revision: 9_000_000,
            ..StateSnapshot::default()
        });
        store.clear();
        assert!(
            store
                .apply(StateSnapshot {
                    revision: 3,
                    ..StateSnapshot::default()
                })
                .is_some(),
            "a new gateway starts its own revision sequence"
        );
    }

    #[test]
    fn an_equal_revision_is_accepted_as_a_refresh() {
        let mut store = StateStore::default();
        let candidate = StateSnapshot {
            revision: 7,
            ..StateSnapshot::default()
        };
        assert!(store.apply(candidate.clone()).is_some());
        assert!(store.apply(candidate).is_some());
    }
}
