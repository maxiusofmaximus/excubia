//! «Tu equipo» in Equipo: one status line per hired professional, in the order the host
//! sends it. The desktop shows the status as it comes; it never reads one from the headline.

use serde::{Deserialize, Serialize};

use super::error::ChatResult;
use crate::client::HostClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamMemberStatus {
    WaitingForYou,
    Blocked,
    Working,
    Done,
    Idle,
}

/// One professional as the host summarises them. `headline` is the host's own fixed
/// sentence; `since` is an ISO time the screen turns into an age.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamMemberSummary {
    pub agent_id: String,
    pub name: String,
    pub role_title: String,
    pub status: TeamMemberStatus,
    pub headline: String,
    pub since: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamSummary {
    pub members: Vec<TeamMemberSummary>,
}

impl HostClient {
    /// Reads the status of the team. An empty team is an empty list, not a failure.
    pub async fn team_summary(&self) -> ChatResult<TeamSummary> {
        self.chat_get("/v1/team/summary", &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_reads_the_members_in_the_order_the_host_sends_them() {
        let body = r#"{"ok":true,"members":[
            {"agent_id":"hire_2","name":"Ana","role_title":"Contable","status":"waiting_for_you","headline":"Espera tu visto bueno.","since":"2026-11-05T11:40:00.000Z"},
            {"agent_id":"hire_1","name":"Bob","role_title":"Programador","status":"idle","headline":"Sin actividad","since":"2026-11-05T10:00:00.000Z"}]}"#;
        let summary: TeamSummary = serde_json::from_str(body).expect("summary");
        assert_eq!(summary.members[0].agent_id, "hire_2");
        assert_eq!(summary.members[0].status, TeamMemberStatus::WaitingForYou);
        assert_eq!(summary.members[1].status, TeamMemberStatus::Idle);
        assert_eq!(summary.members[1].role_title, "Programador");
    }

    #[test]
    fn every_status_the_contract_names_is_read() {
        for (text, expected) in [
            ("waiting_for_you", TeamMemberStatus::WaitingForYou),
            ("blocked", TeamMemberStatus::Blocked),
            ("working", TeamMemberStatus::Working),
            ("done", TeamMemberStatus::Done),
            ("idle", TeamMemberStatus::Idle),
        ] {
            let parsed: TeamMemberStatus =
                serde_json::from_str(&format!("\"{text}\"")).expect("status");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn a_status_the_contract_does_not_name_is_a_read_error() {
        assert!(serde_json::from_str::<TeamMemberStatus>("\"sleeping\"").is_err());
    }

    #[test]
    fn an_empty_team_is_a_summary_with_no_members_not_an_error() {
        let summary: TeamSummary =
            serde_json::from_str(r#"{"ok":true,"members":[]}"#).expect("empty");
        assert!(summary.members.is_empty());
    }
}
