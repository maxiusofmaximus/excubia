//! A mailbox rule (F3B) is one question for the operator's device alone: no lead,
//! CTO, worker or allow rule decides it, however it is worded. It waits the ordinary
//! ten minutes, and the generic path cannot file it.

mod common;

use std::sync::Arc;

use common::{assert_for_the_operator_alone, assert_no_agent_decides, team, Team};
use excubia_chat::{
    action_digest, ApprovalKind, ChatError, ManualClock, NewApproval, Reader, DEFAULT_TTL_SECONDS,
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

/// One rule that moves a sender's mail to junk. The name is part of the content.
fn rule(team: &Team, name: &str, ttl_seconds: u32) -> NewApproval {
    let args = json!({
        "account": "trabajo",
        "name": name,
        "conditions": {"from_addresses": ["alguien@example.com"]},
        "actions": {"move_to_folder": "junkemail"},
    });
    question(team, "mail.rule", args, ttl_seconds)
}

#[test]
fn a_rule_waits_for_the_operator_and_no_agent_decides_it() {
    let team = team("rule-operator");
    let approval = team
        .store
        .create_mail_rule_approval(rule(&team, "Facturas", DEFAULT_TTL_SECONDS))
        .expect("the rule is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);
}

/// An allow rule that names `mail_rule` is refused when the file loads (an allow rule
/// cannot cover a never-delegable category), so the last good policy stays and the
/// rule is still asked. The guard that holds with or without the file is
/// `policy/rules/policy.rs`, which escalates `never_delegate` before any rule.
#[test]
fn an_allow_rule_cannot_cover_a_rule() {
    let team = team("rule-allow");
    std::fs::write(
        team.dir.join("approvals.toml"),
        r#"version = 1

[[rule]]
id = "rules-free"
decision = "allow"
category = ["mail_rule"]
cwd_under = "any"
"#,
    )
    .expect("write rules");

    let approval = team
        .store
        .create_mail_rule_approval(rule(&team, "Facturas", DEFAULT_TTL_SECONDS))
        .expect("the rule is filed");

    assert_for_the_operator_alone(&approval);
}

#[test]
fn a_rule_may_wait_ten_minutes_and_no_longer() {
    let team = team("rule-ttl");
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock);

    let asked = team
        .store
        .create_mail_rule_approval(rule(&team, "Facturas", 1800))
        .expect("the rule is filed");

    assert_eq!(asked.expires_at, "2026-10-21T00:10:00.000Z");
}

#[test]
fn a_rule_cannot_be_filed_by_the_generic_path() {
    let team = team("rule-forged");
    let mut forged = rule(&team, "Falsa", DEFAULT_TTL_SECONDS);
    forged.tool = "mail.rule".to_owned();

    let refused = team
        .store
        .create_approval(forged)
        .expect_err("the host places mail rules, not the caller");

    assert!(
        matches!(refused, ChatError::ApprovalScopeDenied),
        "{refused:?}"
    );
}

#[test]
fn only_the_operator_sees_the_rule_it_asks_about() {
    let team = team("rule-view");
    let args = json!({
        "account": "trabajo",
        "name": "Copia",
        "conditions": {"from_addresses": ["alguien@example.com"]},
        "actions": {"move_to_folder": "junkemail"},
    });
    let approval = team
        .store
        .create_mail_rule_approval(question(&team, "mail.rule", args.clone(), 300))
        .expect("the rule is filed");

    assert_eq!(approval.view(Reader::Operator)["details"], args);
    let view = approval.view(Reader::Other);
    assert!(view.get("details").is_none(), "{view}");
    assert!(view.get("payload_redacted").is_none(), "{view}");
    assert!(!view.to_string().contains("alguien@example.com"), "{view}");
}
