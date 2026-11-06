//! F3 timeline: a plan leaves one line when it is proposed and one when it is
//! decided, numbered in the same `seq` order as the messages and approvals.

mod common;

use common::{team, Team};
use hn_core_chat::{
    action_digest, ApprovalKind, NewApproval, NewPlan, PlanDecision, PlanStep, TimelineEntry,
    Touch, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

fn four_steps() -> Vec<PlanStep> {
    (0..4)
        .map(|index| PlanStep {
            title: format!("Paso {index}"),
            touches: vec![Touch::Files],
        })
        .collect()
}

fn plan_lines(t: &Team) -> Vec<TimelineEntry> {
    t.store
        .timeline(&t.conversation, None, 200)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "plan")
        .collect()
}

#[test]
fn a_plan_leaves_one_line_when_proposed_and_one_when_decided() {
    let t = team("plan-timeline-lines");
    let plan = t
        .store
        .propose_plan(NewPlan {
            conversation_id: t.conversation.clone(),
            proposer_agent_id: t.lead.clone(),
            steps: four_steps(),
        })
        .expect("plan");
    t.store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "user")
        .expect("approve");

    let lines = plan_lines(&t);
    let statuses: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.status.as_deref())
        .collect();
    assert_eq!(statuses, ["proposed", "approved"]);
    assert!(lines
        .iter()
        .all(|line| line.ref_id.as_deref() == Some(plan.id.as_str())));
    assert!(lines[0].seq < lines[1].seq);
    assert_eq!(lines[0].decider_kind, None);
    assert_eq!(lines[1].decider_kind.as_deref(), Some("user"));
    assert_eq!(
        lines[1].summary.as_deref(),
        Some("Plan de 4 pasos · lo propone Ernesto · toca: ficheros")
    );
}

#[test]
fn plans_and_approvals_share_one_seq_order_with_no_repeats() {
    let t = team("plan-timeline-seq");
    let args = json!({ "command": "ls" });
    let cwd = t.root.to_string_lossy().to_string();
    t.store
        .create_approval(NewApproval {
            conversation_id: Some(t.conversation.clone()),
            seat_id: Some(t.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "Ejecutar comando: ls".to_string(),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval");
    let plan = t
        .store
        .propose_plan(NewPlan {
            conversation_id: t.conversation.clone(),
            proposer_agent_id: t.lead.clone(),
            steps: four_steps(),
        })
        .expect("plan");
    t.store
        .decide_plan(&plan.id, PlanDecision::Reject, None, "user")
        .expect("reject");

    let entries = t
        .store
        .timeline(&t.conversation, None, 200)
        .expect("timeline")
        .entries;
    let mut seqs: Vec<i64> = entries.iter().map(|entry| entry.seq).collect();
    let total = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(seqs.len(), total, "no two lines share a seq");
    assert!(entries.iter().any(|entry| entry.kind == "approval"));
    assert_eq!(plan_lines(&t).len(), 2);
}
