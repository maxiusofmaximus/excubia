//! F3 store rules: a delegation of more steps than the threshold must start
//! under an approved plan, a plan that touches mail or the network is never
//! approved by rule, and a changed plan is proposed again.

mod common;

use common::{team, Team};
use excubia_chat::{
    ChatError, Delegation, NewDelegation, NewPlan, NewSeat, PlanDecision, PlanStatus, PlanStep,
    Touch,
};

fn step(title: &str, touches: &[Touch]) -> PlanStep {
    PlanStep {
        title: title.to_string(),
        touches: touches.to_vec(),
    }
}

fn steps(count: usize, touches: &[Touch]) -> Vec<PlanStep> {
    (0..count)
        .map(|index| step(&format!("Paso {index}"), touches))
        .collect()
}

fn propose(t: &Team, steps: Vec<PlanStep>) -> excubia_chat::Plan {
    t.store
        .propose_plan(NewPlan {
            conversation_id: t.conversation.clone(),
            proposer_agent_id: t.lead.clone(),
            steps,
        })
        .expect("plan")
}

/// Alfred gives the lead its task: the parent every lead delegation hangs from.
fn lead_parent(t: &Team) -> Delegation {
    t.assign(&t.alfred, &t.lead, "Equipo", "run_root", None)
        .expect("the lead's task")
}

/// The lead gives `to` a delegation of `steps` steps under `parent`, optionally
/// under a plan. `label` keeps the tasks apart.
fn assign_steps(
    t: &Team,
    parent: &Delegation,
    label: &str,
    to: &str,
    steps: u32,
    plan_id: Option<String>,
) -> Result<Delegation, ChatError> {
    t.store.create_delegation(NewDelegation {
        conversation_id: t.conversation.clone(),
        from_agent_id: t.lead.clone(),
        to_agent_id: to.to_string(),
        task: format!("Montar {label}"),
        turn_key: parent.id.clone(),
        parent_id: Some(parent.id.clone()),
        steps,
        plan_id,
    })
}

fn write_policy(t: &Team, text: &str) {
    std::fs::write(t.dir.join("approvals.toml"), text).expect("write policy");
}

#[test]
fn a_plan_of_four_steps_waits_for_the_approval_and_then_links_its_delegation() {
    let t = team("plan-happy");
    let parent = lead_parent(&t);
    let plan = propose(&t, steps(4, &[Touch::Files]));
    assert_eq!(plan.status, PlanStatus::Proposed);
    assert_eq!(
        plan.summary,
        "Plan de 4 pasos · lo propone Ernesto · toca: ficheros"
    );

    let approved = t
        .store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "user")
        .expect("approve");
    assert_eq!(approved.status, PlanStatus::Approved);
    assert_eq!(approved.decided_by.as_deref(), Some("user"));

    let delegation = assign_steps(
        &t,
        &parent,
        "feliz",
        &t.specialist,
        4,
        Some(plan.id.clone()),
    )
    .expect("delegation");
    let linked = t.store.plan(&plan.id).expect("plan");
    assert_eq!(
        linked.delegation_id.as_deref(),
        Some(delegation.id.as_str())
    );
}

#[test]
fn a_delegation_of_more_steps_than_the_threshold_is_blocked_until_its_plan_is_approved() {
    let t = team("plan-blocked");
    let parent = lead_parent(&t);
    let plan = propose(&t, steps(4, &[Touch::Files]));

    let error = assign_steps(
        &t,
        &parent,
        "sin-aprobar",
        &t.specialist,
        4,
        Some(plan.id.clone()),
    )
    .expect_err("not approved");
    assert!(matches!(error, ChatError::PlanNotApproved), "{error}");
    let error = assign_steps(&t, &parent, "sin-plan", &t.specialist, 4, None).expect_err("no plan");
    assert!(matches!(error, ChatError::PlanRequired), "{error}");

    t.store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "user")
        .expect("approve");
    assign_steps(
        &t,
        &parent,
        "aprobado",
        &t.specialist,
        4,
        Some(plan.id.clone()),
    )
    .expect("now it starts");

    // A plan starts one delegation only: a second one is refused.
    let bruno = t
        .store
        .create_seat(NewSeat {
            project_id: t.project.clone(),
            name: "Bruno".to_string(),
            role_id: "specialist".to_string(),
        })
        .expect("second specialist");
    let error = assign_steps(
        &t,
        &parent,
        "repetido",
        &bruno.agent_id,
        4,
        Some(plan.id.clone()),
    )
    .expect_err("already linked");
    assert!(matches!(error, ChatError::PlanLinked), "{error}");
}

#[test]
fn the_threshold_is_three_by_default_and_four_steps_need_a_plan() {
    let t = team("plan-frontier");
    let parent = lead_parent(&t);
    let error =
        assign_steps(&t, &parent, "cuatro", &t.specialist, 4, None).expect_err("four need a plan");
    assert!(matches!(error, ChatError::PlanRequired), "{error}");
    assign_steps(&t, &parent, "tres", &t.specialist, 3, None).expect("three steps need no plan");
}

#[test]
fn the_threshold_comes_from_the_policy_file() {
    let t = team("plan-threshold");
    let parent = lead_parent(&t);
    write_policy(&t, "version = 1\n[plans]\nthreshold = 2\n");
    let error =
        assign_steps(&t, &parent, "umbral", &t.specialist, 3, None).expect_err("threshold 2");
    assert!(matches!(error, ChatError::PlanRequired), "{error}");
}

#[test]
fn a_safe_plan_is_approved_by_rule_only_when_the_rule_is_on() {
    let t = team("plan-auto");
    let parent = lead_parent(&t);
    write_policy(&t, "version = 1\n[plans]\nauto_approve_safe = true\n");
    let safe = propose(&t, steps(4, &[Touch::Files, Touch::Tools]));
    assert_eq!(safe.status, PlanStatus::Approved);
    assert_eq!(safe.decided_by.as_deref(), Some("policy"));
    assign_steps(&t, &parent, "regla", &t.specialist, 4, Some(safe.id))
        .expect("the rule approved it");
}

#[test]
fn a_plan_with_mail_or_network_is_never_approved_by_rule() {
    let t = team("plan-auto-mail");
    write_policy(&t, "version = 1\n[plans]\nauto_approve_safe = true\n");
    let mail = propose(&t, steps(4, &[Touch::Files, Touch::Mail]));
    let network = propose(&t, steps(4, &[Touch::Network]));
    assert_eq!(mail.status, PlanStatus::Proposed);
    assert_eq!(network.status, PlanStatus::Proposed);
}

#[test]
fn a_plan_with_changes_is_proposed_again_and_the_old_one_stays_final() {
    let t = team("plan-changes");
    let first = propose(&t, steps(4, &[Touch::Files]));
    let asked = t
        .store
        .decide_plan(
            &first.id,
            PlanDecision::RequestChanges,
            Some("Quita el paso 3".to_string()),
            "user",
        )
        .expect("changes");
    assert_eq!(asked.status, PlanStatus::ChangesRequested);
    assert_eq!(asked.note.as_deref(), Some("Quita el paso 3"));

    let again = t
        .store
        .decide_plan(&first.id, PlanDecision::Approve, None, "user")
        .expect_err("a decided plan is final");
    assert!(matches!(again, ChatError::PlanDecided), "{again}");

    let second = propose(&t, steps(3, &[Touch::Files]));
    assert_eq!(second.status, PlanStatus::Proposed);
    let pending = t.store.plans(Some(PlanStatus::Proposed)).expect("pending");
    let ids: Vec<&str> = pending.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, [second.id.as_str()]);
}

#[test]
fn an_invalid_plan_is_refused_whole() {
    let t = team("plan-invalid");
    let long = "x".repeat(121);
    let cases = [
        (steps(0, &[Touch::Files]), "no steps"),
        (steps(21, &[Touch::Files]), "21 steps"),
        (vec![step(&long, &[Touch::Files])], "title over 120"),
        (vec![step("<b>Borra</b>", &[Touch::Files])], "html"),
        (vec![step("Haz `rm -rf`", &[Touch::Files])], "backticks"),
        (vec![step("Sin tocar nada", &[])], "no touches"),
    ];
    for (bad, why) in cases {
        let error = t
            .store
            .propose_plan(NewPlan {
                conversation_id: t.conversation.clone(),
                proposer_agent_id: t.lead.clone(),
                steps: bad,
            })
            .expect_err(why);
        assert!(matches!(error, ChatError::InvalidPlan(_)), "{why}: {error}");
    }
    assert!(t.store.plans(None).expect("list").is_empty());
}

#[test]
fn a_change_request_note_is_at_most_300_characters() {
    let t = team("plan-note");
    let plan = propose(&t, steps(4, &[Touch::Files]));
    let error = t
        .store
        .decide_plan(
            &plan.id,
            PlanDecision::RequestChanges,
            Some("n".repeat(301)),
            "user",
        )
        .expect_err("note too long");
    assert!(matches!(error, ChatError::InvalidPlan(_)), "{error}");
}

#[test]
fn a_declared_count_of_one_does_not_hold_back_a_four_step_plan() {
    let t = team("plan-declared-ignored");
    let parent = lead_parent(&t);
    let plan = propose(&t, steps(4, &[Touch::Files]));
    t.store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "user")
        .expect("approve");

    let delegation = assign_steps(&t, &parent, "uno", &t.specialist, 1, Some(plan.id.clone()))
        .expect("the plan decides, not the declared one");
    let linked = t.store.plan(&plan.id).expect("plan");
    assert_eq!(
        linked.delegation_id.as_deref(),
        Some(delegation.id.as_str())
    );
}

#[test]
fn a_declared_count_that_disagrees_with_the_plan_loses_to_the_plan() {
    let t = team("plan-count-loses");
    let parent = lead_parent(&t);
    let plan = propose(&t, steps(2, &[Touch::Files]));
    t.store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "user")
        .expect("approve");

    assign_steps(
        &t,
        &parent,
        "dice-nueve",
        &t.specialist,
        9,
        Some(plan.id.clone()),
    )
    .expect("the two-step plan decides, not the declared nine");
}
