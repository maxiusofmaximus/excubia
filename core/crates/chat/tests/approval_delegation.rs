//! Approvals decided by the seats of the team (CORE9-B): a lead decides the
//! requests of its project's workers, the CTO decides its own level, nobody
//! decides its own request, and every decision leaves its audit row.

mod common;

use common::{team, Team};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalStatus, ChatError, Decision, HookScope,
    NewApproval, NewProject, SeatActor, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

/// The decisions one agent may make in an hour (`approval_delegation.rs`).
const HOURLY_DECISIONS: usize = 6;

fn actor(agent_id: &str, role_id: &str, project_id: Option<&str>, team: &Team) -> SeatActor {
    SeatActor {
        agent_id: agent_id.to_string(),
        role_id: role_id.to_string(),
        project_id: project_id.map(str::to_string),
        conversation_id: team.conversation.clone(),
        run_id: String::new(),
    }
}

fn lead_of(team: &Team) -> SeatActor {
    actor(&team.lead, "lead", Some(&team.project), team)
}

fn specialist_of(team: &Team) -> SeatActor {
    actor(&team.specialist, "specialist", Some(&team.project), team)
}

fn alfred_of(team: &Team) -> SeatActor {
    actor(&team.alfred, "alfred", None, team)
}

/// A command asked by `seat` from the project root. The residents of the team
/// have an empty run id, which is what `run_id` carries here.
fn try_ask(
    team: &Team,
    seat: &str,
    command: &str,
    run_id: Option<&str>,
) -> Result<Approval, ChatError> {
    let args = json!({ "command": command });
    let cwd = team.root.to_string_lossy().to_string();
    team.store.create_approval(NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: Some(seat.to_string()),
        agent_label: Some("ana".to_string()),
        run_id: run_id.map(str::to_string),
        pane_id: None,
        kind: ApprovalKind::Command,
        tool: "bash".to_string(),
        summary: format!("Ejecutar comando: {command}"),
        cwd: Some(cwd.clone()),
        paths: Vec::new(),
        risk: None,
        action_digest: action_digest("command", "bash", &args, &cwd, &[]),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    })
}

fn ask(team: &Team, seat: &str, command: &str) -> Approval {
    try_ask(team, seat, command, Some("")).expect("approval")
}

fn decide(
    team: &Team,
    decider: &SeatActor,
    approval: &Approval,
    decision: Decision,
) -> Result<Approval, ChatError> {
    team.store
        .decide_delegated(decider, &approval.id, decision, None)
}

/// The audit rows of one approval: decider kind, decider id, level, decision.
fn audit_rows(team: &Team, approval_id: &str) -> Vec<(String, String, Option<String>, String)> {
    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    let mut statement = connection
        .prepare(
            "SELECT decider_kind, decider_id, level, decision
               FROM approval_audit WHERE approval_id = ?1 ORDER BY id",
        )
        .expect("audit query");
    statement
        .query_map([approval_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("audit rows")
        .map(|row| row.expect("audit row"))
        .collect()
}

#[test]
fn the_lead_approves_its_worker_and_the_action_runs_once() {
    let team = team("delegation-once");
    let approval = ask(&team, &team.specialist, "npm install left-pad");
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(approval.reason.as_deref(), Some("no_rule"));

    let decided = decide(&team, &lead_of(&team), &approval, Decision::Approve).expect("decision");
    assert_eq!(decided.status, ApprovalStatus::Approved);

    let scope = HookScope::seat(&team.specialist);
    let digest = Some(approval.action_digest.as_str());
    let consumed = team
        .store
        .consume_approval(&approval.id, digest, &scope)
        .expect("the action runs");
    assert_eq!(consumed.status, ApprovalStatus::Consumed);
    let again = team.store.consume_approval(&approval.id, digest, &scope);
    assert!(matches!(again, Err(ChatError::ApprovalAlreadyDecided)));

    let rows = audit_rows(&team, &approval.id);
    assert_eq!(rows.len(), 2, "one row for the decision, one for the burn");
    assert_eq!(
        rows[0],
        (
            "agent".to_string(),
            team.lead.clone(),
            Some("lead".to_string()),
            "approved".to_string()
        )
    );
    assert_eq!(rows[1].3, "consumed");
}

#[test]
fn a_seat_never_decides_its_own_request() {
    let team = team("delegation-self");
    let approval = ask(&team, &team.lead, "npm install left-pad");
    let refused = decide(&team, &lead_of(&team), &approval, Decision::Approve);
    assert!(matches!(refused, Err(ChatError::ApprovalSelfApproval)));
    let still = team.store.approval(&approval.id).expect("read");
    assert_eq!(still.status, ApprovalStatus::Pending);
}

#[test]
fn a_lead_does_not_decide_the_workers_of_another_project() {
    let team = team("delegation-scope");
    let other = team
        .store
        .create_project(NewProject {
            name: "Otra".to_string(),
            root_path: team.root.with_file_name("otra"),
        })
        .expect("second project");
    let other_lead = other.lead.as_ref().expect("its lead").agent_id.clone();
    let approval = ask(&team, &team.specialist, "npm install left-pad");

    let stranger = actor(&other_lead, "lead", Some(&other.id), &team);
    let refused = decide(&team, &stranger, &approval, Decision::Approve);
    assert!(matches!(refused, Err(ChatError::ApprovalScopeDenied)));

    let decided = decide(&team, &lead_of(&team), &approval, Decision::Approve);
    assert!(decided.is_ok(), "its own lead still decides it");
}

#[test]
fn a_push_is_the_users_and_no_seat_decides_it() {
    let team = team("delegation-push");
    let approval = ask(&team, &team.specialist, "git push origin main");
    assert_eq!(approval.level.as_deref(), Some("user"));

    let by_lead = decide(&team, &lead_of(&team), &approval, Decision::Approve);
    assert!(matches!(by_lead, Err(ChatError::ApprovalNotDelegable)));
    let by_alfred = decide(&team, &alfred_of(&team), &approval, Decision::Approve);
    assert!(matches!(by_alfred, Err(ChatError::ApprovalNotDelegable)));
}

#[test]
fn the_cto_decides_its_level_and_a_lead_never_decides_it() {
    let team = team("delegation-cto");
    let approval = ask(&team, &team.specialist, "npm install left-pad");

    let not_yet = decide(&team, &alfred_of(&team), &approval, Decision::Approve);
    assert!(matches!(not_yet, Err(ChatError::ApprovalLevelMismatch)));

    // Only the CTO level (created by the escalation of a later phase) reaches Alfred.
    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    connection
        .execute(
            "UPDATE approval SET level = 'cto' WHERE id = ?1",
            [&approval.id],
        )
        .expect("raise the level");

    let by_lead = decide(&team, &lead_of(&team), &approval, Decision::Approve);
    assert!(matches!(by_lead, Err(ChatError::ApprovalLevelMismatch)));
    let by_alfred = decide(&team, &alfred_of(&team), &approval, Decision::Approve)
        .expect("the CTO decides its level");
    assert_eq!(by_alfred.status, ApprovalStatus::Approved);
    assert_eq!(audit_rows(&team, &approval.id)[0].2.as_deref(), Some("cto"));
}

#[test]
fn a_worker_cannot_approve_anything_and_its_request_stays_pending() {
    let team = team("delegation-worker");
    let approval = ask(&team, &team.specialist, "npm install left-pad");

    let own = decide(&team, &specialist_of(&team), &approval, Decision::Approve);
    assert!(matches!(own, Err(ChatError::ApprovalScopeDenied)));
    let read = team.store.approval(&approval.id).expect("read");
    assert_eq!(read.status, ApprovalStatus::Pending);
    assert!(audit_rows(&team, &approval.id).is_empty());
}

#[test]
fn a_lead_decides_six_approvals_an_hour_and_the_seventh_is_refused() {
    let team = team("delegation-budget");
    let lead = lead_of(&team);
    for index in 0..=HOURLY_DECISIONS {
        let approval = ask(&team, &team.specialist, &format!("npm install pkg-{index}"));
        let result = decide(&team, &lead, &approval, Decision::Approve);
        if index < HOURLY_DECISIONS {
            assert!(result.is_ok(), "decision {}", index + 1);
        } else {
            assert!(matches!(result, Err(ChatError::ApprovalBudget)));
        }
    }
}

#[test]
fn the_same_action_asked_again_by_its_seat_is_the_same_pending_approval() {
    let team = team("delegation-coalesce");
    let first = ask(&team, &team.specialist, "npm install left-pad");
    let second = ask(&team, &team.specialist, "npm install left-pad");
    assert_eq!(first.id, second.id);
    assert_eq!(team.store.pending_approval_count().expect("count"), 1);

    // Another seat asking for the same digest is a different request.
    let other = try_ask(&team, &team.lead, "npm install left-pad", None);
    assert!(matches!(other, Err(ChatError::ApprovalDigestReused)));

    decide(&team, &lead_of(&team), &first, Decision::Deny).expect("the lead denies it");
    let spent = try_ask(&team, &team.specialist, "npm install left-pad", Some(""));
    assert!(matches!(spent, Err(ChatError::ApprovalDigestReused)));
}

#[test]
fn an_approval_that_expires_is_decided_by_the_system() {
    let team = team("delegation-expiry");
    let args = json!({ "command": "npm install short-lived" });
    let cwd = team.root.to_string_lossy().to_string();
    let approval = team
        .store
        .create_approval(NewApproval {
            conversation_id: Some(team.conversation.clone()),
            seat_id: Some(team.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: "Ejecutar comando: npm install short-lived".to_string(),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: 1,
        })
        .expect("approval");

    std::thread::sleep(std::time::Duration::from_millis(1500));
    let expired = team.store.approval(&approval.id).expect("read");
    assert_eq!(expired.status, ApprovalStatus::Expired);
    let rows = audit_rows(&team, &approval.id);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "system");
    assert_eq!(rows[0].3, "expired");
}
