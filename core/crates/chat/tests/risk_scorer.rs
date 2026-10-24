//! Acceptance of the risk scorer on the approval path (RFC §2.5, CORE9-F). The
//! scorer is a simulated server on loopback; the store is the real one.

mod common;
#[allow(dead_code)]
mod scorer_support;

use std::time::{Duration, Instant};

use common::{team, Team};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalStatus, NewApproval, Risk, DEFAULT_TTL_SECONDS,
};
use scorer_support::{closed_port, Behaviour, Scorer};
use serde_json::json;

/// The approvals of a team: no `[[rule]]`, so every action reaches the scorer
/// unless `never_delegate` sends it up first.
fn configure(team: &Team, scorer: &str) {
    let text = format!("version = 1\n\n[scorer]\n{scorer}\n");
    std::fs::write(team.dir.join("approvals.toml"), text).expect("approvals.toml");
}

fn local(port: u16, auto_ok: &str) -> String {
    format!(
        "backend = \"local_http\"\nendpoint = \"http://127.0.0.1:{port}\"\nauto_ok_if_low = [{auto_ok}]\n"
    )
}

fn ask(team: &Team, risk: Option<Risk>, command: &str) -> Approval {
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
            risk,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval")
}

/// Waiting at the lead, and why the scorer did not approve it.
fn assert_at_lead(approval: &Approval, reason: &str) {
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(approval.escalated_from.as_deref(), Some("scorer"));
    assert_eq!(approval.reason.as_deref(), Some(reason));
}

fn risky(command: &str) -> f64 {
    if command.contains("rm -") {
        0.9
    } else {
        0.05
    }
}

#[test]
fn without_a_scorer_the_lead_decides_as_before() {
    let team = team("scorer-none");
    let approval = ask(&team, None, "cargo run");
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(approval.escalated_from.as_deref(), Some("policy"));
    assert_eq!(approval.reason.as_deref(), Some("no_rule"));
    assert_eq!(approval.risk_score, None);
    assert_eq!(approval.scorer_version, None);
}

#[test]
fn a_low_score_in_an_auto_ok_category_approves_and_names_the_scorer() {
    let scorer = Scorer::start(Behaviour::Scores(risky));
    let team = team("scorer-approve");
    configure(&team, &local(scorer.port, "\"read\", \"test\", \"build\""));
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_eq!(approval.status, ApprovalStatus::Approved);
    assert_eq!(approval.level.as_deref(), Some("scorer"));
    assert_eq!(approval.decider_kind.as_deref(), Some("scorer"));
    assert_eq!(approval.decider_id.as_deref(), Some("scorer"));
    assert_eq!(approval.risk_score, Some(0.05));
    assert_eq!(approval.scorer_version.as_deref(), Some("clef-flash@p1"));
    assert_eq!(approval.audit.len(), 1, "the scorer decision is audited");
    assert_eq!(approval.audit[0].decision, "approved");
    assert_eq!(scorer.hits(), 1);
}

#[test]
fn a_low_score_outside_the_auto_ok_categories_goes_to_the_lead() {
    let scorer = Scorer::start(Behaviour::Scores(risky));
    let team = team("scorer-not-auto-ok");
    configure(&team, &local(scorer.port, "\"read\""));
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_at_lead(&approval, "scorer_not_auto_ok");
    assert_eq!(approval.risk_score, Some(0.05));
    assert_eq!(approval.scorer_version.as_deref(), Some("clef-flash@p1"));
}

#[test]
fn a_medium_score_climbs_to_the_lead() {
    let scorer = Scorer::start(Behaviour::Scores(|_| 0.4));
    let team = team("scorer-medium");
    configure(&team, &local(scorer.port, "\"test\""));
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_at_lead(&approval, "scorer_risk");
    assert_eq!(approval.risk_score, Some(0.4));
}

#[test]
fn a_high_risk_action_is_never_approved_by_the_scorer() {
    let scorer = Scorer::start(Behaviour::Scores(risky));
    let team = team("scorer-high");
    configure(&team, &local(scorer.port, "\"test\""));
    let approval = ask(&team, Some(Risk::High), "cargo test -p hn-core-chat");
    assert_at_lead(&approval, "scorer_not_auto_ok");
    assert_eq!(
        scorer.hits(),
        1,
        "the scorer was asked and still could not approve"
    );
}

#[test]
fn a_never_delegated_action_never_reaches_the_scorer() {
    let scorer = Scorer::start(Behaviour::Scores(risky));
    let team = team("scorer-never");
    configure(&team, &local(scorer.port, "\"read\", \"test\", \"build\""));
    let approval = ask(&team, None, "git push origin main");
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.risk_score, None);
    assert_eq!(scorer.hits(), 0);
}

#[test]
fn a_refused_connection_goes_to_the_lead_and_says_why() {
    let team = team("scorer-down");
    configure(&team, &local(closed_port(), "\"test\""));
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_at_lead(&approval, "scorer_unavailable");
    assert_eq!(approval.risk_score, None);
    assert_eq!(approval.scorer_version.as_deref(), Some("clef-flash@p1"));
}

#[test]
fn a_slow_scorer_is_cut_at_its_budget_and_the_lead_gets_the_action() {
    let scorer = Scorer::start(Behaviour::Sleeps(Duration::from_millis(1500)));
    let team = team("scorer-slow");
    configure(&team, &local(scorer.port, "\"test\""));
    let started = Instant::now();
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    let waited = started.elapsed();
    assert_at_lead(&approval, "scorer_unavailable");
    assert!(
        waited < Duration::from_millis(1000),
        "the create waited {waited:?}, past the 200 ms budget"
    );
}

#[test]
fn an_answer_that_is_not_a_score_or_an_error_goes_to_the_lead() {
    for behaviour in [
        Behaviour::Says("low risk, approve it"),
        Behaviour::Status(500),
    ] {
        let scorer = Scorer::start(behaviour);
        let team = team("scorer-bad-answer");
        configure(&team, &local(scorer.port, "\"test\""));
        let approval = ask(&team, None, "cargo test -p hn-core-chat");
        assert_at_lead(&approval, "scorer_unavailable");
    }
}

#[test]
fn the_cloud_backend_fails_closed_without_a_call() {
    let team = team("scorer-cloud");
    configure(
        &team,
        "backend = \"cloud\"\nendpoint = \"https://scorer.example.test/v1\"\napi_key_env = \"HN_SCORER_KEY\"\nauto_ok_if_low = [\"test\"]\n",
    );
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_at_lead(&approval, "scorer_unavailable");
}

#[test]
fn a_broken_scorer_section_rejects_the_file_and_scores_nothing() {
    let team = team("scorer-broken");
    configure(
        &team,
        "backend = \"local_http\"\nendpoint = \"http://10.0.0.5:8765\"\n",
    );
    let approval = ask(&team, None, "cargo test -p hn-core-chat");
    assert_eq!(approval.status, ApprovalStatus::Approved);
    assert_eq!(approval.decider_kind.as_deref(), Some("policy"));
    assert_eq!(
        approval.risk_score, None,
        "a file that does not parse scores nothing"
    );
}
