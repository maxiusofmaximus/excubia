//! A batch of mail moves (F2b-1) is one question for the operator's device
//! alone: no lead, CTO, worker or allow rule decides it. A refused batch stays
//! refused until it changes, and only a batch may wait up to 1800 seconds.

mod common;

use std::sync::Arc;

use common::{assert_for_the_operator_alone, assert_no_agent_decides, team, Team};
use hn_core_chat::{
    action_digest, ApprovalKind, ApprovalStatus, ChatError, Decision, ManualClock, NewApproval,
    Reader, DEFAULT_TTL_SECONDS,
};
use serde_json::{json, Value};

/// 2026-10-21T00:00:00.000Z, the instant every test starts from.
const START_MS: i64 = 1_792_540_800_000;

/// A question for `tool` with `args` as its payload, asked from the project root.
fn question(team: &Team, tool: &str, args: Value, ttl_seconds: u32) -> NewApproval {
    let cwd = team.root.to_string_lossy().to_string();
    let paths: Vec<String> = Vec::new();
    NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: tool.to_string(),
        summary: format!("Acción {tool}"),
        cwd: Some(cwd.clone()),
        paths: paths.clone(),
        risk: None,
        action_digest: action_digest("tool", tool, &args, &cwd, &paths),
        payload_redacted: Some(args),
        ttl_seconds,
    }
}

/// A shell command the specialist asks from the project root, as the agents do.
fn command(team: &Team, line: &str, ttl_seconds: u32) -> NewApproval {
    NewApproval {
        seat_id: Some(team.specialist.clone()),
        kind: ApprovalKind::Command,
        ..question(team, "bash", json!({ "command": line }), ttl_seconds)
    }
}

/// A batch moving two mails to `junkemail`: another subject is other content.
fn batch(team: &Team, subject: &str, ttl_seconds: u32) -> NewApproval {
    let args = json!({
        "cuenta": "trabajo",
        "destino": "junkemail",
        "ids": ["msg-1", "msg-2"],
        "asunto": subject,
    });
    question(team, "mail.move", args, ttl_seconds)
}

#[test]
fn a_batch_waits_for_the_operator_and_no_agent_decides_it() {
    let team = team("organize-operator");
    let approval = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", DEFAULT_TTL_SECONDS))
        .expect("the batch is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);
}

#[test]
fn an_allow_rule_does_not_decide_a_batch() {
    let team = team("organize-rule");
    std::fs::write(
        team.dir.join("approvals.toml"),
        r#"version = 1

[[rule]]
id = "local-work"
decision = "allow"
category = ["read", "edit", "test", "build"]
cwd_under = "any"
"#,
    )
    .expect("write rules");

    let read = team
        .store
        .create_approval(command(&team, "cat notes.md", 120))
        .expect("the read is filed");
    assert_eq!(read.status, ApprovalStatus::Approved, "the rule is live");

    let approval = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", DEFAULT_TTL_SECONDS))
        .expect("the batch is filed");
    assert_for_the_operator_alone(&approval);
}

#[test]
fn a_refused_batch_is_not_asked_again_until_it_changes() {
    let team = team("organize-refused");
    let first = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", DEFAULT_TTL_SECONDS))
        .expect("the batch is filed");
    team.store
        .decide_approval(&first.id, Decision::Deny, "device-1", "device-1")
        .expect("the device refuses");

    let refused = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", DEFAULT_TTL_SECONDS))
        .expect_err("the same batch is refused");
    assert!(
        matches!(refused, ChatError::ApprovalDigestRefused),
        "{refused:?}"
    );

    let changed = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza v2", DEFAULT_TTL_SECONDS))
        .expect("a changed batch is asked");
    assert_eq!(changed.status, ApprovalStatus::Pending);
}

#[test]
fn an_expired_batch_can_be_asked_again() {
    let team = team("organize-expired");
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock.clone());
    let first = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", 1800))
        .expect("the batch is filed");

    clock.advance_secs(1801);
    let again = team
        .store
        .create_mail_organize_approval(batch(&team, "Limpieza", 1800))
        .expect("the same batch is asked again after it expired");

    assert_ne!(again.id, first.id);
    assert_eq!(again.status, ApprovalStatus::Pending);
}

#[test]
fn only_the_operator_sees_the_moves_of_a_batch() {
    let team = team("organize-view");
    let args = json!({ "cuenta": "trabajo", "destino": "junkemail", "ids": ["msg-1"] });
    let approval = team
        .store
        .create_mail_organize_approval(question(&team, "mail.move", args.clone(), 120))
        .expect("the batch is filed");

    assert_eq!(approval.view(Reader::Operator)["details"], args);
    let view = approval.view(Reader::Other);
    assert!(view.get("details").is_none(), "{view}");
    assert!(view.get("payload_redacted").is_none(), "{view}");
    assert!(!view.to_string().contains("msg-1"), "{view}");
}

#[test]
fn a_batch_may_wait_thirty_minutes_and_no_longer() {
    let team = team("organize-ttl");
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock.clone());

    let thirty = team
        .store
        .create_mail_organize_approval(batch(&team, "Lote 30", 1800))
        .expect("the batch is filed");
    assert_eq!(thirty.expires_at, "2026-10-21T00:30:00.000Z");

    let clamped = team
        .store
        .create_mail_organize_approval(batch(&team, "Lote 31", 1801))
        .expect("the batch is filed");
    assert_eq!(clamped.expires_at, "2026-10-21T00:30:00.000Z");
}

#[test]
fn every_other_question_keeps_the_five_minute_ceiling() {
    let team = team("organize-ttl-others");
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock.clone());

    let send = team
        .store
        .create_mail_send_approval(question(
            &team,
            "mail.send",
            json!({ "cuenta": "trabajo", "borrador": "draft-1" }),
            1800,
        ))
        .expect("the send is filed");
    assert_eq!(send.expires_at, "2026-10-21T00:05:00.000Z");

    let shell = team
        .store
        .create_approval(command(&team, "npm test", 1800))
        .expect("the command is filed");
    assert_eq!(shell.expires_at, "2026-10-21T00:05:00.000Z");
}

#[test]
fn a_mail_action_cannot_be_filed_by_the_generic_path() {
    let team = team("organize-forged");
    for tool in ["mail.move", "mail.send", "mail.attach_path"] {
        let mut forged = batch(&team, "Falso", DEFAULT_TTL_SECONDS);
        forged.tool = tool.to_owned();
        let refused = team
            .store
            .create_approval(forged)
            .expect_err("the host places mail actions, not the caller");
        assert!(matches!(refused, ChatError::ApprovalScopeDenied), "{tool}");
    }
}
