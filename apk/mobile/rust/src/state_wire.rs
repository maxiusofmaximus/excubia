//! Wire shapes and lenient decoding. Kept out of `api` so the bridge only sees
//! the stable client model, not the gateway's JSON internals.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::api::state::{AgentEntry, AgentStatus, StateSnapshot};

#[derive(Debug, Deserialize)]
pub(crate) struct SseEnvelope {
    pub(crate) state: Option<WireState>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireState {
    #[serde(default)]
    revision: u64,
    herdr_version: Option<String>,
    #[serde(default)]
    counts: BTreeMap<String, u64>,
    #[serde(default)]
    agents: Vec<WireAgent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WireAgent {
    pane_id: Option<String>,
    workspace_id: Option<String>,
    project: Option<String>,
    agent: Option<String>,
    status: Option<String>,
    attention: Option<u8>,
    title: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    can_resume: bool,
    #[serde(default)]
    restored: bool,
    state_change_seq: Option<u64>,
}

pub(crate) fn parse_state(json: &str) -> Option<StateSnapshot> {
    serde_json::from_str::<WireState>(json).ok().map(sanitise)
}

pub(crate) fn parse_sse_state(data: &str) -> Option<StateSnapshot> {
    let envelope: SseEnvelope = serde_json::from_str(data).ok()?;
    envelope.state.map(sanitise)
}

pub(crate) fn revision_of(data: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    value
        .get("revision")?
        .as_u64()
        .map(|revision| revision.min(u32::MAX as u64) as u32)
}

pub(crate) fn state_change_seq_of(data: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    value
        .get("state_change_seq")?
        .as_u64()
        .map(|seq| seq.min(u32::MAX as u64) as u32)
}

fn sanitise(wire: WireState) -> StateSnapshot {
    let agents: Vec<AgentEntry> = wire.agents.into_iter().filter_map(sanitise_agent).collect();

    let counts = if wire.counts.is_empty() {
        derive_counts(&agents)
    } else {
        wire.counts
            .into_iter()
            .filter(|(status, _)| AgentStatus::from_known(status).is_some())
            .map(|(status, count)| (status, count.min(u32::MAX as u64) as u32))
            .collect()
    };

    StateSnapshot {
        revision: wire.revision.min(u32::MAX as u64) as u32,
        herdr_version: non_empty(wire.herdr_version),
        counts,
        agents,
    }
}

fn sanitise_agent(agent: WireAgent) -> Option<AgentEntry> {
    let pane_id = agent.pane_id?.trim().to_string();
    if pane_id.is_empty() {
        return None;
    }
    let workspace_id = non_empty(agent.workspace_id)
        .unwrap_or_else(|| pane_id.split(':').next().unwrap_or_default().to_string());
    let project = non_empty(agent.project)
        .or_else(|| Some(workspace_id.clone()).filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "?".to_string());
    let status = AgentStatus::parse(agent.status.as_deref());
    Some(AgentEntry {
        pane_id,
        workspace_id,
        project,
        agent: non_empty(agent.agent).unwrap_or_else(|| "agent".to_string()),
        status,
        attention: agent.attention.unwrap_or_else(|| status.attention()),
        title: non_empty(agent.title),
        cwd: non_empty(agent.cwd),
        focused: agent.focused,
        can_resume: agent.can_resume,
        restored: agent.restored,
        state_change_seq: agent
            .state_change_seq
            .map(|seq| seq.min(u32::MAX as u64) as u32),
    })
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn derive_counts(agents: &[AgentEntry]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for agent in agents {
        *counts
            .entry(agent.status.wire_name().to_string())
            .or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_and_seq_come_from_the_frame_envelope() {
        assert_eq!(revision_of(r#"{"revision":42}"#), Some(42));
        assert_eq!(revision_of(r#"{"event":"ping"}"#), None);
        assert_eq!(revision_of("not json"), None);
        assert_eq!(state_change_seq_of(r#"{"state_change_seq":9}"#), Some(9));
        assert_eq!(state_change_seq_of(r#"{"revision":9}"#), None);
    }

    #[test]
    fn a_revision_above_u32_is_clamped_not_wrapped() {
        assert_eq!(revision_of(r#"{"revision":5000000000}"#), Some(u32::MAX));
    }
}
