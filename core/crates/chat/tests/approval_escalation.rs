//! The escalation of an approval on a clock the test moves (CORE9-C): a level
//! that runs out climbs, the TTL expires it, a restart closes what waits, and
//! an action that is never delegated goes straight to the operator.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{team, Team};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalStatus, ChatError, ChatStore, Decision,
    HookScope, ManualClock, NewApproval, SeatActor, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

/// 2026-10-21T00:00:00.000Z, the instant every test starts from.
const START_MS: i64 = 1_792_540_800_000;

/// A team whose approvals run on a clock the test moves by hand.
fn timed(name: &str) -> (Team, Arc<ManualClock>) {
    let team = team(name);
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock.clone());
    (team, clock)
}

/// A command asked by the specialist from the project root.
fn ask_for(team: &Team, command: &str, ttl_seconds: u32) -> Approval {
    let args = json!({ "command": command });
    let cwd = team.root.to_string_lossy().to_string();
    team.store
        .create_approval(NewApproval {
            conversation_id: Some(team.conversation.clone()),
            seat_id: Some(team.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds,
        })
        .expect("approval")
}

/// A command the policy does not cover: it waits at the lead for 40 s.
fn ask(team: &Team, command: &str) -> Approval {
    ask_for(team, command, DEFAULT_TTL_SECONDS)
}

fn read(team: &Team, approval_id: &str) -> Approval {
    team.store.approval(approval_id).expect("approval")
}

fn lead_actor(team: &Team) -> SeatActor {
    SeatActor {
        agent_id: team.lead.clone(),
        role_id: "lead".to_string(),
        project_id: Some(team.project.clone()),
        conversation_id: team.conversation.clone(),
        run_id: String::new(),
    }
}

/// Set the status of the seats of one role, as the host does when a panel stops.
fn set_status(team: &Team, role_id: &str, status: &str) {
    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    connection
        .execute(
            "UPDATE agent_seat SET status = ?1 WHERE role_id = ?2",
            [status, role_id],
        )
        .expect("seat status");
}

/// The audit trail of one approval: decider kind, decider id, level, decision.
fn audit_of(db: &Path, approval_id: &str) -> Vec<(String, String, Option<String>, String)> {
    let connection = rusqlite::Connection::open(db).expect("raw database");
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

fn host(level: Option<&str>, decision: &str) -> (String, String, Option<String>, String) {
    (
        "system".to_string(),
        "host".to_string(),
        level.map(str::to_string),
        decision.to_string(),
    )
}

#[test]
fn a_level_that_runs_out_climbs_and_the_ttl_expires_it() {
    let (team, clock) = timed("chain");
    let approval = ask(&team, "npm install left-pad");
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(approval.escalated_from.as_deref(), Some("policy"));

    clock.advance_secs(39);
    assert_eq!(read(&team, &approval.id).level.as_deref(), Some("lead"));

    clock.advance_secs(1);
    let cto = read(&team, &approval.id);
    assert_eq!(cto.level.as_deref(), Some("cto"));
    assert_eq!(cto.escalated_from.as_deref(), Some("lead"));
    assert_eq!(cto.reason.as_deref(), Some("timeout"));

    clock.advance_secs(40);
    let user = read(&team, &approval.id);
    assert_eq!(user.level.as_deref(), Some("user"));
    assert_eq!(user.escalated_from.as_deref(), Some("cto"));
    assert_eq!(user.status, ApprovalStatus::Pending);

    clock.advance_secs(119 - 80);
    assert_eq!(read(&team, &approval.id).status, ApprovalStatus::Pending);

    clock.advance_secs(1);
    let expired = read(&team, &approval.id);
    assert_eq!(expired.status, ApprovalStatus::Expired);
    assert_eq!(expired.decider_kind.as_deref(), Some("system"));
    assert_eq!(expired.decider_id.as_deref(), Some("host"));

    // The chain is monotone and ends in the expiry: no step back, no repeat.
    assert_eq!(
        audit_of(&team.db, &approval.id),
        vec![
            host(Some("cto"), "escalated"),
            host(Some("user"), "escalated"),
            host(None, "expired"),
        ]
    );
}

#[test]
fn a_decision_in_time_stops_the_clock() {
    let (team, clock) = timed("in-time");
    let approval = ask(&team, "npm install left-pad");

    clock.advance_secs(30);
    let decided = team
        .store
        .decide_delegated(&lead_actor(&team), &approval.id, Decision::Approve, None)
        .expect("the lead decides in time");
    assert_eq!(decided.status, ApprovalStatus::Approved);

    clock.advance_secs(200);
    let later = read(&team, &approval.id);
    assert_eq!(later.status, ApprovalStatus::Approved);
    assert_eq!(later.level.as_deref(), Some("lead"));
    assert_eq!(
        audit_of(&team.db, &approval.id),
        vec![(
            "agent".to_string(),
            team.lead.clone(),
            Some("lead".to_string()),
            "approved".to_string(),
        )]
    );
}

#[test]
fn an_operator_who_answers_after_the_ttl_finds_it_expired_and_the_hook_sees_it() {
    let (team, clock) = timed("late");
    let approval = ask(&team, "npm install left-pad");

    clock.advance_secs(120);
    let late = team
        .store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1");
    assert!(matches!(late, Err(ChatError::ApprovalExpired)));

    let state = team
        .store
        .approval_state(&approval.id, &HookScope::seat(&team.specialist))
        .expect("the hook reads its approval");
    assert_eq!(state.status, ApprovalStatus::Expired);
}

#[test]
fn a_restart_expires_what_waits_and_audits_it_as_the_host() {
    let (team, _clock) = timed("restart");
    let approval = ask(&team, "npm install left-pad");
    let db = team.db.clone();
    // The scratch directory outlives the store: the test reopens the database in it.
    let Team {
        store: live,
        dir: _scratch,
        ..
    } = team;
    drop(live);

    let store = ChatStore::open(&db).expect("reopen the store");
    let reopened = store.approval(&approval.id).expect("approval");
    assert_eq!(reopened.status, ApprovalStatus::Expired);
    assert_eq!(reopened.decider_kind.as_deref(), Some("system"));
    assert_eq!(reopened.decider_id.as_deref(), Some("host"));
    assert_eq!(
        audit_of(&db, &approval.id).last(),
        Some(&host(None, "expired"))
    );
}

#[test]
fn a_git_push_is_never_delegated_and_goes_straight_to_the_user() {
    let (team, _clock) = timed("push");
    let approval = ask_for(&team, "git push origin main", DEFAULT_TTL_SECONDS);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.escalated_from.as_deref(), Some("policy"));
    assert_eq!(approval.reason.as_deref(), Some("never_delegate"));

    let by_lead =
        team.store
            .decide_delegated(&lead_actor(&team), &approval.id, Decision::Approve, None);
    assert!(matches!(by_lead, Err(ChatError::ApprovalNotDelegable)));
    assert_eq!(read(&team, &approval.id).status, ApprovalStatus::Pending);
}

#[test]
fn a_lead_with_no_live_seat_is_skipped_for_the_cto() {
    let (team, clock) = timed("no-lead");
    set_status(&team, "lead", "stopped");

    let approval = ask(&team, "npm install left-pad");
    assert_eq!(approval.level.as_deref(), Some("cto"));
    assert_eq!(approval.escalated_from.as_deref(), Some("policy"));
    assert_eq!(approval.reason.as_deref(), Some("decider_unavailable"));

    clock.advance_secs(40);
    let user = read(&team, &approval.id);
    assert_eq!(user.level.as_deref(), Some("user"));
    assert_eq!(user.reason.as_deref(), Some("timeout"));
}

#[test]
fn with_no_live_lead_and_no_cto_the_approval_goes_straight_to_the_user() {
    let (team, clock) = timed("no-agents");
    set_status(&team, "lead", "stopped");
    set_status(&team, "alfred", "stopped");

    let approval = ask(&team, "npm install left-pad");
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.reason.as_deref(), Some("decider_unavailable"));

    clock.advance_secs(119);
    assert_eq!(read(&team, &approval.id).status, ApprovalStatus::Pending);
    clock.advance_secs(1);
    assert_eq!(read(&team, &approval.id).status, ApprovalStatus::Expired);
}

#[test]
fn a_ttl_under_fifteen_seconds_goes_straight_to_the_user() {
    let (team, _clock) = timed("short-ttl");
    let approval = ask_for(&team, "npm install left-pad", 10);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.reason.as_deref(), Some("no_rule"));
    assert_eq!(approval.status, ApprovalStatus::Pending);
}

#[test]
fn the_expiry_leaves_its_line_in_the_timeline() {
    let (team, clock) = timed("line");
    let approval = ask(&team, "npm install left-pad");

    clock.advance_secs(120);
    let page = team
        .store
        .timeline(&team.conversation, None, 50)
        .expect("timeline");
    let lines: Vec<_> = page
        .entries
        .iter()
        .filter(|entry| entry.kind == "approval")
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].ref_id.as_deref(), Some(approval.id.as_str()));
    assert_eq!(lines[0].status.as_deref(), Some("expired"));
    assert_eq!(lines[0].decider_kind.as_deref(), Some("system"));
    assert_eq!(lines[0].decider_id.as_deref(), Some("host"));
    assert_eq!(
        lines[0].summary.as_deref(),
        Some("Host expiró: Ejecutar comando: npm install left-pad")
    );
}
