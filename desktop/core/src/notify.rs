//! Transition detection for system notifications.
//!
//! Alerts are driven by applied state, never by polling: the SSE loop already
//! delivers every revision, and this tracker turns a real status change into
//! one alert per pane and revision.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::state::{AgentStatus, StateSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentAlert {
    pub pane_id: String,
    pub project: String,
    pub title: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Copy)]
struct Seen {
    status: AgentStatus,
    revision: u64,
}

#[derive(Debug, Default)]
pub struct TransitionTracker {
    seen: HashMap<String, Seen>,
    primed: bool,
}

impl TransitionTracker {
    /// The alerts a snapshot produces. The first snapshot only records the
    /// baseline: agents already blocked before the app connected are not news.
    pub fn observe(&mut self, state: &StateSnapshot) -> Vec<AgentAlert> {
        let mut alerts = Vec::new();
        let mut present = HashSet::with_capacity(state.agents.len());

        for agent in &state.agents {
            present.insert(agent.pane_id.clone());
            let previous = self.seen.get(&agent.pane_id).copied();
            let recorded = previous
                .is_some_and(|seen| seen.status == agent.status && seen.revision == state.revision);
            let transitioned = previous.is_none_or(|seen| seen.status != agent.status);
            let alerting = matches!(agent.status, AgentStatus::Blocked | AgentStatus::Done);

            if self.primed && alerting && transitioned && !recorded {
                alerts.push(AgentAlert {
                    pane_id: agent.pane_id.clone(),
                    project: agent.project.clone().unwrap_or_else(|| "?".to_string()),
                    title: agent
                        .title
                        .clone()
                        .or_else(|| agent.agent.clone())
                        .unwrap_or_else(|| "Sin tarea".to_string()),
                    status: agent.status,
                });
            }

            self.seen.insert(
                agent.pane_id.clone(),
                Seen {
                    status: agent.status,
                    revision: state.revision,
                },
            );
        }

        self.seen.retain(|pane, _| present.contains(pane));
        self.primed = true;
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentRow;

    fn snapshot(revision: u64, agents: &[(&str, AgentStatus)]) -> StateSnapshot {
        StateSnapshot {
            revision,
            agents: agents
                .iter()
                .map(|(pane_id, status)| AgentRow {
                    pane_id: pane_id.to_string(),
                    project: Some("proyecto".to_string()),
                    title: Some("tarea".to_string()),
                    status: *status,
                    ..AgentRow::default()
                })
                .collect(),
            ..StateSnapshot::default()
        }
    }

    #[test]
    fn the_first_snapshot_only_primes_the_baseline() {
        let mut tracker = TransitionTracker::default();
        let state = snapshot(1, &[("w1:p1", AgentStatus::Blocked)]);
        assert!(tracker.observe(&state).is_empty());
        assert!(
            tracker.observe(&state).is_empty(),
            "the baseline is never re-alerted"
        );
    }

    #[test]
    fn a_real_transition_alerts_once_per_revision() {
        let mut tracker = TransitionTracker::default();
        tracker.observe(&snapshot(1, &[("w1:p1", AgentStatus::Idle)]));

        let blocked = snapshot(2, &[("w1:p1", AgentStatus::Blocked)]);
        let alerts = tracker.observe(&blocked);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].pane_id, "w1:p1");
        assert_eq!(alerts[0].status, AgentStatus::Blocked);
        assert!(
            tracker.observe(&blocked).is_empty(),
            "the same revision must not alert twice"
        );
        assert!(
            tracker
                .observe(&snapshot(3, &[("w1:p1", AgentStatus::Blocked)]))
                .is_empty(),
            "staying blocked is not a new transition"
        );

        tracker.observe(&snapshot(4, &[("w1:p1", AgentStatus::Working)]));
        let again = tracker.observe(&snapshot(5, &[("w1:p1", AgentStatus::Blocked)]));
        assert_eq!(again.len(), 1, "a later transition alerts again");
    }

    #[test]
    fn done_also_alerts_and_working_never_does() {
        let mut tracker = TransitionTracker::default();
        tracker.observe(&snapshot(1, &[("w1:p1", AgentStatus::Working)]));
        assert!(tracker
            .observe(&snapshot(2, &[("w1:p1", AgentStatus::Working)]))
            .is_empty());
        let alerts = tracker.observe(&snapshot(3, &[("w1:p1", AgentStatus::Done)]));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].status, AgentStatus::Done);
    }

    #[test]
    fn a_pane_that_disappears_and_returns_is_news_again() {
        let mut tracker = TransitionTracker::default();
        tracker.observe(&snapshot(1, &[("w1:p1", AgentStatus::Idle)]));
        assert_eq!(
            tracker
                .observe(&snapshot(2, &[("w1:p1", AgentStatus::Blocked)]))
                .len(),
            1
        );
        tracker.observe(&snapshot(3, &[]));
        assert_eq!(
            tracker
                .observe(&snapshot(4, &[("w1:p1", AgentStatus::Blocked)]))
                .len(),
            1,
            "a pane that left and came back blocked is a new alert"
        );
    }

    #[test]
    fn a_new_pane_that_starts_blocked_alerts() {
        let mut tracker = TransitionTracker::default();
        tracker.observe(&snapshot(1, &[("w1:p1", AgentStatus::Idle)]));
        let alerts = tracker.observe(&snapshot(
            2,
            &[
                ("w1:p1", AgentStatus::Idle),
                ("w1:p2", AgentStatus::Blocked),
            ],
        ));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].pane_id, "w1:p2");
    }
}
