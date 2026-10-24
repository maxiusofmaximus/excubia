//! The state read model, decoded leniently (`docs/contracts/state.md`).
//!
//! The gateway ranks and computes attention; this module only carries those
//! values through, dropping rows that cannot be addressed and degrading an
//! unknown status instead of failing.

use std::collections::HashMap;

use crate::state_wire;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Blocked,
    Working,
    Done,
    Idle,
    Unknown,
}

impl AgentStatus {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        value.and_then(Self::from_known).unwrap_or(Self::Unknown)
    }

    pub(crate) fn from_known(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "blocked" => Some(Self::Blocked),
            "working" => Some(Self::Working),
            "done" => Some(Self::Done),
            "idle" => Some(Self::Idle),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    pub(crate) fn attention(self) -> u8 {
        match self {
            Self::Blocked => 3,
            Self::Done => 2,
            Self::Working | Self::Unknown => 1,
            Self::Idle => 0,
        }
    }

    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Working => "working",
            Self::Done => "done",
            Self::Idle => "idle",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub pane_id: String,
    pub workspace_id: String,
    pub project: String,
    pub agent: String,
    pub status: AgentStatus,
    pub attention: u8,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub focused: bool,
    pub can_resume: bool,
    pub restored: bool,
    pub state_change_seq: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSnapshot {
    pub revision: u32,
    pub herdr_version: Option<String>,
    pub counts: HashMap<String, u32>,
    pub agents: Vec<AgentEntry>,
}

/// Decode `GET /v1/state`; None when the body is not a state object, so the
/// caller can keep the last good state instead of emptying the screen.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_state(json: String) -> Option<StateSnapshot> {
    state_wire::parse_state(&json)
}

/// Decode the nested `state` object of the opening SSE frame.
pub(crate) fn parse_sse_state(data: &str) -> Option<StateSnapshot> {
    state_wire::parse_sse_state(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Option<StateSnapshot> {
        parse_state(json.to_string())
    }

    #[test]
    fn parses_the_contract_state_and_carries_the_ranking_through() {
        let state = parse(
            r#"{
                "revision": 42,
                "herdr_version": "0.9.3",
                "focused": {"workspace_id": "w1", "pane_id": "w1:p1"},
                "counts": {"blocked": 1, "working": 2, "unrecognised": 9},
                "agents": [
                    {"pane_id": "w1:p1", "workspace_id": "w1", "project": "herdr-notify",
                     "agent": "opencode", "status": "blocked", "attention": 3,
                     "title": "Add the pairing screen", "cwd": "/repo",
                     "can_resume": true, "future_field": "ignored"}
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(state.revision, 42);
        assert_eq!(state.herdr_version.as_deref(), Some("0.9.3"));
        let agent = &state.agents[0];
        assert_eq!(agent.pane_id, "w1:p1");
        assert_eq!(agent.status, AgentStatus::Blocked);
        assert_eq!(agent.attention, 3);
        assert!(agent.can_resume);
        assert_eq!(state.counts.get("blocked"), Some(&1));
        assert_eq!(state.counts.get("working"), Some(&2));
        assert!(!state.counts.contains_key("unrecognised"));
    }

    #[test]
    fn a_row_without_a_pane_id_is_dropped() {
        let state = parse(
            r#"{"revision":1,"agents":[
                {"workspace_id":"w1"},
                {"pane_id":"w1:p2","restored":true,"state_change_seq":7}
            ]}"#,
        )
        .unwrap();
        assert_eq!(state.agents.len(), 1);
        assert_eq!(state.agents[0].pane_id, "w1:p2");
        assert!(state.agents[0].restored);
        assert_eq!(state.agents[0].state_change_seq, Some(7));
    }

    #[test]
    fn project_and_agent_fall_back_like_the_contracts_say() {
        let state = parse(
            r#"{"revision":1,"agents":[
                {"pane_id":"w1:p1","workspace_id":"w1"},
                {"pane_id":"w2:p1","project":"   "}
            ]}"#,
        )
        .unwrap();
        assert_eq!(state.agents[0].project, "w1");
        assert_eq!(state.agents[0].agent, "agent");
        assert_eq!(
            state.agents[1].project, "w2",
            "no project and no workspace: the pane prefix is the workspace"
        );
    }

    #[test]
    fn an_unknown_status_degrades_and_derives_attention() {
        let state = parse(
            r#"{"revision":1,"agents":[
                {"pane_id":"w1:p1","status":"exploded"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(state.agents[0].status, AgentStatus::Unknown);
        assert_eq!(state.agents[0].attention, 1);
    }

    #[test]
    fn counts_are_derived_when_an_older_gateway_omits_them() {
        let state = parse(
            r#"{"revision":1,"agents":[
                {"pane_id":"w1:p1","status":"blocked"},
                {"pane_id":"w1:p2","status":"idle"},
                {"pane_id":"w1:p3","status":"idle"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(state.counts.get("blocked"), Some(&1));
        assert_eq!(state.counts.get("idle"), Some(&2));
    }

    #[test]
    fn a_malformed_body_is_none_not_a_crash() {
        assert!(parse("not json").is_none());
        assert!(parse("[]").is_none());
        assert!(parse_sse_state("{\"event\":\"state\"}").is_none());
    }

    #[test]
    fn the_opening_frame_carries_the_same_state_object() {
        let state = parse_sse_state(
            r#"{"event":"state","revision":7,
                "state":{"revision":7,"agents":[{"pane_id":"w9:p3"}]}}"#,
        )
        .unwrap();
        assert_eq!(state.revision, 7);
        assert_eq!(state.agents[0].pane_id, "w9:p3");
    }
}
