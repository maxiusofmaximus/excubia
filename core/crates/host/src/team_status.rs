//! `status.overview`: Alfred's read of the company in one call. The panels come
//! from the state `GET /v1/state` serves, the rest from the chat. The answer
//! carries labels and counts, never a path, a token, a payload or a pane id.

use std::collections::{BTreeMap, HashMap, HashSet};

use hn_core_chat::{
    ProjectSummary, SeatActor, StatusApproval, StatusDelegation, StatusFacts, ToolError, ToolReply,
};
use hn_core_state::{StatePayload, TriageRow};
use serde_json::{json, Value};

use super::team::TeamService;
use super::team_errors::tool_chat_error;

/// The most items one list of the overview carries. The rest is counted and
/// marked `truncated`, so the answer stays small whatever the company holds.
const OVERVIEW_ITEMS: usize = 20;

impl TeamService {
    pub(super) async fn status_overview(&self, actor: &SeatActor) -> Result<ToolReply, ToolError> {
        let state = self.host.state_payload().await;
        let facts = self.chat.status_facts(actor).map_err(tool_chat_error)?;
        Ok(ToolReply {
            result: overview(&state, &facts),
        })
    }
}

/// The answer of `status.overview`, from one state and one chat read.
pub(super) fn overview(state: &StatePayload, facts: &StatusFacts) -> Value {
    json!({
        "revision": state.revision,
        "chat_revision": facts.chat_revision,
        "panels": panels(state, facts),
        "delegations": delegations(&facts.delegations),
        "approvals": approvals(&facts.approvals),
        "last_activity": facts.last_activity,
    })
}

fn panels(state: &StatePayload, facts: &StatusFacts) -> Value {
    let shown = &facts.projects[..facts.projects.len().min(OVERVIEW_ITEMS)];
    let project_of: HashMap<&str, &str> = facts
        .panes
        .iter()
        .map(|(pane, project)| (pane.as_str(), project.as_str()))
        .collect();
    let listed: HashSet<&str> = shown.iter().map(|project| project.id.as_str()).collect();
    let mut by_project: HashMap<&str, Vec<&TriageRow>> = HashMap::new();
    let mut unassigned = Vec::new();
    for row in &state.agents {
        match project_of.get(row.pane_id.as_str()).copied() {
            Some(project) if listed.contains(project) => {
                by_project.entry(project).or_default().push(row)
            }
            _ => unassigned.push(row),
        }
    }
    let projects: Vec<Value> = shown
        .iter()
        .map(|project| {
            let rows = by_project.get(project.id.as_str());
            project_panels(project, rows.map_or(&[][..], Vec::as_slice))
        })
        .collect();
    json!({
        "total": state.agents.len(),
        "counts": tally(state.agents.iter()),
        // Panels not under a listed project: no seat, an archived project, or
        // a project past the cap.
        "unassigned": tally(unassigned.iter().copied()),
        "projects": projects,
        "truncated": facts.projects.len() > OVERVIEW_ITEMS,
    })
}

fn project_panels(project: &ProjectSummary, rows: &[&TriageRow]) -> Value {
    json!({
        "name": project.name,
        "root_label": project.root_label,
        "counts": tally(rows.iter().copied()),
    })
}

/// Panels per status, under the same keys `/v1/state` counts them with.
fn tally<'a>(rows: impl IntoIterator<Item = &'a TriageRow>) -> Value {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for row in rows {
        *counts.entry(row.status.as_str()).or_default() += 1;
    }
    json!(counts)
}

fn delegations(items: &[StatusDelegation]) -> Value {
    json!({
        "open": items.len(),
        "items": items
            .iter()
            .take(OVERVIEW_ITEMS)
            .map(|delegation| json!({
                "id": delegation.id,
                "state": delegation.state.as_str(),
                "task": delegation.task,
                "to": delegation.to_name,
            }))
            .collect::<Vec<_>>(),
        "truncated": items.len() > OVERVIEW_ITEMS,
    })
}

fn approvals(items: &[StatusApproval]) -> Value {
    json!({
        "pending": items.len(),
        "items": items
            .iter()
            .take(OVERVIEW_ITEMS)
            .map(|approval| json!({
                "id": approval.id,
                "kind": approval.kind.as_str(),
                "agent": approval.agent,
            }))
            .collect::<Vec<_>>(),
        "truncated": items.len() > OVERVIEW_ITEMS,
    })
}

#[cfg(test)]
mod tests {
    use hn_core_chat::{ApprovalKind, DelegationState};
    use hn_core_state::Status;

    use super::*;

    fn row(pane_id: &str, status: Status, cwd: &str) -> TriageRow {
        TriageRow {
            pane_id: pane_id.to_string(),
            workspace_id: "ws_secret".to_string(),
            project: "excubia".to_string(),
            agent: "claude".to_string(),
            status,
            attention: status.attention(),
            title: None,
            cwd: cwd.to_string(),
            focused: false,
            can_resume: false,
            restored: false,
        }
    }

    fn state(agents: Vec<TriageRow>) -> StatePayload {
        let mut counts = HashMap::new();
        for agent in &agents {
            *counts.entry(agent.status.as_str().to_string()).or_insert(0) += 1;
        }
        StatePayload {
            revision: 7,
            herdr_version: "test".to_string(),
            focused: None,
            counts,
            agents,
        }
    }

    fn project(id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            id: id.to_string(),
            name: name.to_string(),
            root_label: format!("{name}-root"),
            status: "active".to_string(),
            lead: None,
        }
    }

    fn facts(projects: Vec<ProjectSummary>, panes: &[(&str, &str)]) -> StatusFacts {
        StatusFacts {
            chat_revision: 3,
            projects,
            panes: panes
                .iter()
                .map(|(pane, project)| (pane.to_string(), project.to_string()))
                .collect(),
            delegations: Vec::new(),
            approvals: Vec::new(),
            last_activity: None,
        }
    }

    fn approval(index: usize) -> StatusApproval {
        StatusApproval {
            id: format!("apr_{index}"),
            kind: ApprovalKind::Command,
            agent: "Ana".to_string(),
        }
    }

    #[test]
    fn an_empty_company_answers_an_empty_overview() {
        let answer = overview(&state(Vec::new()), &facts(Vec::new(), &[]));
        assert_eq!(answer["panels"]["total"], 0);
        assert_eq!(answer["panels"]["counts"], json!({}));
        assert_eq!(answer["delegations"]["items"], json!([]));
        assert_eq!(answer["delegations"]["truncated"], false);
        assert_eq!(answer["approvals"]["pending"], 0);
        assert_eq!(answer["last_activity"], Value::Null);
        assert_eq!(answer["chat_revision"], 3);
        assert_eq!(answer["revision"], 7);
    }

    #[test]
    fn the_panel_counts_are_the_state_counts_and_group_by_project() {
        let agents = vec![
            row("pane_a1", Status::Working, "C:\\demo"),
            row("pane_a2", Status::Blocked, "C:\\demo"),
            row("pane_loose", Status::Done, "C:\\loose"),
        ];
        let read = state(agents);
        let answer = overview(
            &read,
            &facts(
                vec![project("prj_a", "demo"), project("prj_b", "otro")],
                &[("pane_a1", "prj_a"), ("pane_a2", "prj_a")],
            ),
        );
        let panels = &answer["panels"];
        assert_eq!(panels["counts"], json!(read.counts));
        assert_eq!(panels["total"], 3);
        assert_eq!(panels["projects"][0]["name"], "demo");
        assert_eq!(panels["projects"][0]["root_label"], "demo-root");
        assert_eq!(
            panels["projects"][0]["counts"],
            json!({ "blocked": 1, "working": 1 })
        );
        assert_eq!(panels["projects"][1]["counts"], json!({}));
        assert_eq!(panels["unassigned"], json!({ "done": 1 }));
    }

    #[test]
    fn a_panel_of_an_unlisted_project_is_unassigned() {
        let read = state(vec![row("pane_old", Status::Idle, "C:\\old")]);
        let answer = overview(&read, &facts(Vec::new(), &[("pane_old", "prj_gone")]));
        assert_eq!(answer["panels"]["unassigned"], json!({ "idle": 1 }));
    }

    #[test]
    fn no_path_token_payload_or_pane_id_reaches_the_answer() {
        let read = state(vec![row(
            "pane_secret_7",
            Status::Working,
            "C:\\secreto\\demo",
        )]);
        let mut chat = facts(
            vec![project("prj_a", "demo")],
            &[("pane_secret_7", "prj_a")],
        );
        chat.delegations.push(StatusDelegation {
            id: "dlg_1".to_string(),
            state: DelegationState::Running,
            task: "Revisa el informe".to_string(),
            to_name: "Ana".to_string(),
        });
        chat.approvals.push(approval(1));
        let text = overview(&read, &chat).to_string();
        for leaked in [
            "pane_secret",
            "secreto",
            "C:\\",
            "root_path",
            "hook_token",
            "payload_redacted",
            "cwd",
            "pane_id",
            "workspace_id",
        ] {
            assert!(!text.contains(leaked), "{leaked} in {text}");
        }
    }

    #[test]
    fn each_list_keeps_twenty_items_and_says_it_truncated() {
        let approvals: Vec<StatusApproval> = (0..25).map(approval).collect();
        let mut chat = facts(Vec::new(), &[]);
        chat.approvals = approvals;
        let answer = overview(&state(Vec::new()), &chat);
        assert_eq!(answer["approvals"]["pending"], 25);
        assert_eq!(
            answer["approvals"]["items"].as_array().map(Vec::len),
            Some(20)
        );
        assert_eq!(answer["approvals"]["truncated"], true);
        assert_eq!(answer["delegations"]["truncated"], false);
    }

    #[test]
    fn the_answer_stays_small_at_the_caps() {
        // Labels at the length `status_facts` bounds them to; tasks at 120.
        let label = "x".repeat(60);
        let mut chat = facts(
            (0..25)
                .map(|index| project(&format!("prj_{index}"), &label))
                .collect(),
            &[],
        );
        chat.delegations = (0..25)
            .map(|index| StatusDelegation {
                id: format!("dlg_{index}"),
                state: DelegationState::Queued,
                task: "t".repeat(120),
                to_name: label.clone(),
            })
            .collect();
        chat.approvals = (0..25).map(approval).collect();
        let text = overview(&state(Vec::new()), &chat).to_string();
        assert!(text.len() < 16 * 1024, "{} bytes", text.len());
    }
}
