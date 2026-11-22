//! A style rule (RFC estilo-aprendido §3) is one question for the operator's device alone:
//! no lead, CTO, worker or allow rule decides it. The category is never delegable, so
//! an allow rule that names it cannot cover it.

mod common;

use common::{assert_for_the_operator_alone, assert_no_agent_decides, team, Team};
use excubia_chat::{action_digest, ApprovalKind, NewApproval, DEFAULT_TTL_SECONDS};
use serde_json::{json, Value};

/// A question for `tool` with `args` as its payload, asked from the project root.
fn question(team: &Team, tool: &str, args: Value) -> NewApproval {
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
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

fn line(team: &Team, rule: &str) -> NewApproval {
    question(
        team,
        "style.propose_rule",
        json!({ "rule": rule, "source": "manual" }),
    )
}

#[test]
fn a_style_rule_waits_for_the_operator_and_no_agent_decides_it() {
    let team = team("style-rule-operator");
    let approval = team
        .store
        .create_style_rule_approval(line(&team, "Frases cortas."))
        .expect("the rule is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);
}

/// An allow rule that names `style_rule` is refused when the file loads, so the last good
/// policy stays and the rule is still asked.
#[test]
fn an_allow_rule_cannot_cover_a_style_rule() {
    let team = team("style-rule-allow");
    std::fs::write(
        team.dir.join("approvals.toml"),
        r#"version = 1

[[rule]]
id = "style-free"
decision = "allow"
category = ["style_rule"]
cwd_under = "any"
"#,
    )
    .expect("write rules");

    let approval = team
        .store
        .create_style_rule_approval(line(&team, "Frases cortas."))
        .expect("the rule is filed");

    assert_for_the_operator_alone(&approval);
}
