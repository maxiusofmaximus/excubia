//! `status.overview`: Alfred's read of the company in one call. The panels come
//! from the state `GET /v1/state` serves, the rest from the chat. The answer
//! carries labels and counts, never a path, a token, a payload or a pane id.

use std::collections::{BTreeMap, HashMap, HashSet};

use excubia_chat::{
    activity_of, status_of, Clock, DelegationState, HireLink, ProjectSummary, SeatActor,
    StatusApproval, StatusDelegation, StatusFacts, SystemClock, TeamStatus, ToolError, ToolReply,
};
use excubia_state::{StatePayload, Status, TriageRow};
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
        let links = self.chat.hire_links().map_err(tool_chat_error)?;
        let mut answer = overview(&state, &facts);
        answer["team"] = team(&state, &links, SystemClock.now_ms());
        answer["hires"] = hires(&links);
        answer["catalog"] = catalog();
        Ok(ToolReply { result: answer })
    }
}

/// Who is hired, by the id `delegation.assign` takes: a hire gets its seat in the
/// conversation's project the first time work goes to it.
fn hires(links: &[HireLink]) -> Value {
    json!(links
        .iter()
        .take(OVERVIEW_ITEMS)
        .map(|link| json!({ "id": link.hire_id, "name": link.name, "role": link.catalog_role }))
        .collect::<Vec<_>>())
}

/// The professions the operator can hire (Equipo → Contratar), so Alfred proposes real
/// roles instead of inventing them. Compiled in; an unreadable catalogue is an empty list.
fn catalog() -> Value {
    let Ok(catalog) = excubia_api::role_catalog::RoleCatalog::embedded() else {
        return json!([]);
    };
    json!(catalog
        .roles
        .iter()
        .map(|role| json!({ "role": role.id, "title": role.title, "area": role.area }))
        .collect::<Vec<_>>())
}

/// The hires per status, read with the same rules as `GET /v1/team/summary`:
/// one decision, `activity_of`, and the pane's own block.
fn team(state: &StatePayload, links: &[HireLink], now_ms: i64) -> Value {
    let blocked: HashSet<&str> = state
        .agents
        .iter()
        .filter(|row| row.status == Status::Blocked)
        .map(|row| row.pane_id.as_str())
        .collect();
    let mut counts: BTreeMap<TeamStatus, u32> =
        TeamStatus::ALL.iter().map(|status| (*status, 0)).collect();
    for link in links {
        let blocked = link
            .seat_pane
            .as_deref()
            .is_some_and(|pane| blocked.contains(pane));
        let (activity, _) = activity_of(link, blocked, now_ms);
        *counts.entry(status_of(&activity)).or_default() += 1;
    }
    json!(counts)
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
    let open: HashSet<&str> = facts
        .delegations
        .iter()
        .map(|delegation| delegation.to_agent_id.as_str())
        .collect();
    let last: HashMap<&str, DelegationState> = facts
        .latest
        .iter()
        .map(|delegation| (delegation.to_agent_id.as_str(), delegation.state))
        .collect();
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
            project_panels(project, rows.map_or(&[][..], Vec::as_slice), &open, &last)
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

fn project_panels(
    project: &ProjectSummary,
    rows: &[&TriageRow],
    open: &HashSet<&str>,
    last: &HashMap<&str, DelegationState>,
) -> Value {
    // Each turn of Alfred is a new session: the id here is how he names the project
    // to agent.spawn and delegation.assign, and the lead says whether anyone runs it.
    json!({
        "id": project.id,
        "name": project.name,
        "root_label": project.root_label,
        "lead": project.lead.as_ref().map(|lead| json!({
            "agent_id": lead.agent_id,
            "name": lead.name,
            "status": lead.status,
            "running": lead.pane_id.is_some(),
            "summary": lead_summary(
                &lead.name,
                open.contains(lead.agent_id.as_str()),
                last.get(lead.agent_id.as_str()).copied(),
            ),
        })),
        "counts": tally(rows.iter().copied()),
    })
}

/// The sentence Alfred repeats about a lead. It reads the lead's work, not its panel: a
/// finished task closes the panel, and that must not read as «detenido».
fn lead_summary(name: &str, open: bool, last: Option<DelegationState>) -> String {
    if open {
        return format!("{name} está trabajando en una tarea.");
    }
    match last {
        Some(DelegationState::Done) => format!("{name} terminó la última tarea."),
        Some(DelegationState::Failed) => format!("{name} no pudo terminar la última tarea."),
        _ => format!("{name} no tiene tareas abiertas."),
    }
}

/// The sentence a tool reply gives about one delegation, with the seat's name and no id.
pub(super) fn delegation_sentence(name: &str, state: DelegationState) -> String {
    match state {
        DelegationState::Done => format!("{name} terminó."),
        DelegationState::Queued | DelegationState::Running | DelegationState::Waiting => {
            format!("{name} aún está con ello.")
        }
        _ => format!("{name} no pudo terminar."),
    }
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
                "summary": delegation_sentence(&delegation.to_name, delegation.state),
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
    use excubia_chat::{ApprovalKind, PendingApproval, SeatSummary};

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
            latest: Vec::new(),
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
            to_agent_id: "ag_ana".to_string(),
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
                to_agent_id: format!("ag_{index}"),
                to_name: label.clone(),
            })
            .collect();
        chat.approvals = (0..25).map(approval).collect();
        let text = overview(&state(Vec::new()), &chat).to_string();
        assert!(text.len() < 16 * 1024, "{} bytes", text.len());
    }

    fn hired(pane: Option<&str>, approval: bool) -> HireLink {
        HireLink {
            hire_id: "hire_1".to_string(),
            name: "Bob".to_string(),
            catalog_role: "bob".to_string(),
            hired_at: "2026-11-05T10:00:00.000Z".to_string(),
            seat_pane: pane.map(str::to_string),
            approval: approval.then(|| PendingApproval {
                kind: ApprovalKind::Edit,
                created_at: "2026-11-05T11:00:00.000Z".to_string(),
            }),
            working_since: None,
            finished_at: None,
            plan: None,
        }
    }

    /// A project whose lead is `name`, with or without a panel.
    fn led(name: &str, pane: Option<&str>) -> ProjectSummary {
        ProjectSummary {
            lead: Some(SeatSummary {
                agent_id: "ag_lead".to_string(),
                name: name.to_string(),
                status: "idle".to_string(),
                uac_state: "unchecked".to_string(),
                pane_id: pane.map(str::to_string),
            }),
            ..project("prj_a", "demo")
        }
    }

    /// A delegation to the lead of `led`.
    fn to_lead(id: &str, state: DelegationState) -> StatusDelegation {
        StatusDelegation {
            id: id.to_string(),
            state,
            task: "Revisa el informe".to_string(),
            to_agent_id: "ag_lead".to_string(),
            to_name: "Ernesto".to_string(),
        }
    }

    fn lead_sentence(answer: &Value) -> String {
        answer["panels"]["projects"][0]["lead"]["summary"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn a_lead_sentence_reads_the_work_not_the_panel() {
        let read = state(Vec::new());
        // Open work, with or without a panel: he is working.
        let mut open = facts(vec![led("Ernesto", None)], &[]);
        open.delegations
            .push(to_lead("dlg_1", DelegationState::Running));
        assert_eq!(
            lead_sentence(&overview(&read, &open)),
            "Ernesto está trabajando en una tarea."
        );
        // The last task done and the panel closed: finished, never «detenido».
        let mut done = facts(vec![led("Ernesto", None)], &[]);
        done.latest.push(to_lead("dlg_0", DelegationState::Done));
        assert_eq!(
            lead_sentence(&overview(&read, &done)),
            "Ernesto terminó la última tarea."
        );
        let mut failed = facts(vec![led("Ernesto", Some("pane_e"))], &[("pane_e", "prj_a")]);
        failed
            .latest
            .push(to_lead("dlg_0", DelegationState::Failed));
        assert_eq!(
            lead_sentence(&overview(&read, &failed)),
            "Ernesto no pudo terminar la última tarea."
        );
        // No delegation at all: no open task, whether the panel is open or not.
        for pane in [None, Some("pane_e")] {
            let none = facts(vec![led("Ernesto", pane)], &[("pane_e", "prj_a")]);
            let answer = overview(&read, &none);
            assert_eq!(lead_sentence(&answer), "Ernesto no tiene tareas abiertas.");
            assert!(!answer.to_string().contains("detenido"));
        }
    }

    #[test]
    fn a_delegation_sentence_names_the_seat_and_no_id() {
        assert_eq!(
            delegation_sentence("Ernesto", DelegationState::Queued),
            "Ernesto aún está con ello."
        );
        assert_eq!(
            delegation_sentence("Ernesto", DelegationState::Done),
            "Ernesto terminó."
        );
        assert_eq!(
            delegation_sentence("Ernesto", DelegationState::Failed),
            "Ernesto no pudo terminar."
        );
    }

    #[test]
    fn each_delegation_item_carries_its_sentence() {
        let mut chat = facts(Vec::new(), &[]);
        chat.delegations = vec![
            to_lead("dlg_1", DelegationState::Running),
            to_lead("dlg_2", DelegationState::Done),
        ];
        let answer = overview(&state(Vec::new()), &chat);
        assert_eq!(
            answer["delegations"]["items"][0]["summary"],
            "Ernesto aún está con ello."
        );
        assert_eq!(
            answer["delegations"]["items"][1]["summary"],
            "Ernesto terminó."
        );
        assert!(!answer["delegations"]["items"][0]["summary"]
            .to_string()
            .contains("dlg_"));
    }

    #[test]
    fn the_team_counts_each_hire_by_the_summary_status() {
        let panes = state(vec![row("pane_b", Status::Blocked, "/x")]);
        let links = [
            hired(None, false),
            hired(Some("pane_b"), false),
            hired(Some("pane_c"), true),
        ];
        let now_ms = 1_793_880_000_000; // 2026-11-05T12:00:00Z

        assert_eq!(
            team(&panes, &links, now_ms),
            json!({ "waiting_for_you": 1, "blocked": 1, "idle": 1,
                    "working": 0, "done": 0 })
        );
    }
}
