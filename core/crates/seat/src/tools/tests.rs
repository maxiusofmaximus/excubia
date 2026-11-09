use hn_core_chat::{AlertMode, NewAlertRule, ToolRequest};

use super::parse::PROGRESS_MAX_CHARS;
use super::*;

use serde_json::{json, Value};

fn request(tool: &str, arguments: Value) -> ToolRequest {
    ToolRequest {
        tool: tool.to_string(),
        arguments,
    }
}

#[test]
fn reply_and_ask_are_typed() {
    assert_eq!(
        parse(&request("chat.reply", json!({"text": "  hola  "}))),
        Ok(ToolCall::Reply {
            text: "hola".to_string(),
            suggestions: Vec::new(),
            attachment_ids: Vec::new(),
        })
    );
    assert_eq!(
        parse(&request(
            "chat.ask_user",
            json!({"question": "¿proyecto?" })
        )),
        Ok(ToolCall::AskUser("¿proyecto?".to_string()))
    );
}

#[test]
fn reply_carries_its_quick_replies_and_refuses_bad_ones() {
    assert_eq!(
        parse(&request(
            "chat.reply",
            json!({"text": "¿Qué nombre?", "suggestions": ["Sí, créalo", "Cámbialo"]})
        )),
        Ok(ToolCall::Reply {
            text: "¿Qué nombre?".to_string(),
            suggestions: vec!["Sí, créalo".to_string(), "Cámbialo".to_string()],
            attachment_ids: Vec::new(),
        })
    );

    let too_many = parse(&request(
        "chat.reply",
        json!({"text": "ok", "suggestions": ["a", "b", "c", "d", "e"]}),
    ))
    .expect_err("five suggestions");
    assert_eq!(too_many.code, "invalid_arguments");
    assert_eq!(too_many.field, Some("suggestions"));

    let too_long = parse(&request(
        "chat.reply",
        json!({"text": "ok", "suggestions": ["a".repeat(61)]}),
    ))
    .expect_err("61 characters");
    assert_eq!(too_long.field, Some("suggestions"));
}

#[test]
fn reply_names_its_files_and_refuses_more_than_four_or_repeats() {
    let named = parse(&request(
        "chat.reply",
        json!({"text": "Aquí tienes", "attachment_ids": ["att_1", "att_2"]}),
    ))
    .expect("two files");
    assert!(matches!(
        named,
        ToolCall::Reply { ref attachment_ids, .. } if attachment_ids == &["att_1", "att_2"]
    ));

    let five = parse(&request(
        "chat.reply",
        json!({"text": "ok", "attachment_ids": ["a", "b", "c", "d", "e"]}),
    ))
    .expect_err("five files");
    assert_eq!(five.field, Some("attachment_ids"));

    let repeated = parse(&request(
        "chat.reply",
        json!({"text": "ok", "attachment_ids": ["att_1", "att_1"]}),
    ))
    .expect_err("a repeated file");
    assert_eq!(repeated.field, Some("attachment_ids"));

    let empty = parse(&request(
        "chat.reply",
        json!({"text": "ok", "attachment_ids": [" "]}),
    ))
    .expect_err("a blank id");
    assert_eq!(empty.field, Some("attachment_ids"));
}

#[test]
fn doc_create_is_typed_and_its_format_is_checked_by_the_parser() {
    let created = parse(&request(
        "doc.create",
        json!({"format": "xlsx", "name": "Gastos", "content": "a,b\n1,2"}),
    ))
    .expect("xlsx");
    assert!(matches!(
        created,
        ToolCall::DocCreate(ref doc) if doc.format == DocFormat::Xlsx && doc.name == "Gastos"
    ));

    let refused = parse(&request(
        "doc.create",
        json!({"format": "exe", "name": "x", "content": "x"}),
    ))
    .expect_err("exe");
    assert_eq!(refused.field, Some("format"));
}

#[test]
fn progress_is_one_plain_line_of_at_most_120_characters() {
    assert_eq!(
        parse(&request(
            "chat.progress",
            json!({"text": "  Revisando   los borradores  "})
        )),
        Ok(ToolCall::Progress("Revisando los borradores".to_string()))
    );
    assert!(parse(&request(
        "chat.progress",
        json!({"text": "a".repeat(PROGRESS_MAX_CHARS)})
    ))
    .is_ok());

    for arguments in [
        json!({"text": "   "}),
        json!({"text": "uno\ndos"}),
        json!({"text": "a".repeat(PROGRESS_MAX_CHARS + 1)}),
        json!({"text": 3}),
        json!({"text": "ok", "percent": 3}),
    ] {
        let refused = parse(&request("chat.progress", arguments.clone())).expect_err("refused");
        assert_eq!(refused.code, "invalid_arguments", "{arguments}");
        assert_eq!(refused.field, Some("text"), "{arguments}");
    }
}

#[test]
fn team_tools_are_typed() {
    assert_eq!(
        parse(&request(
            "project.create",
            json!({"name": "Demo", "path": "demo"})
        )),
        Ok(ToolCall::ProjectCreate(ProjectCreate {
            name: "Demo".to_string(),
            path: Some("demo".to_string()),
        }))
    );
    assert_eq!(
        parse(&request("agent.spawn", json!({"name": "Bob"}))),
        Ok(ToolCall::AgentSpawn(AgentSpawn {
            name: "Bob".to_string(),
            role: "specialist".to_string(),
            project_id: None,
        }))
    );
    assert!(matches!(
        parse(&request(
            "delegation.assign",
            json!({"to_agent_id": "ag_1", "task": "Plan"})
        )),
        Ok(ToolCall::DelegationAssign(_))
    ));
    assert!(matches!(
        parse(&request(
            "delegation.complete",
            json!({"delegation_id": "dlg_1", "result": "Done"})
        )),
        Ok(ToolCall::DelegationComplete(_))
    ));
}

#[test]
fn invalid_arguments_are_rejected_with_a_field() {
    let missing = parse(&request("chat.reply", json!({}))).expect_err("missing text");
    assert_eq!(missing.code, "invalid_arguments");
    assert_eq!(missing.field, Some("text"));

    let wrong_type = parse(&request("chat.reply", json!({"text": 3}))).expect_err("type");
    assert_eq!(wrong_type.code, "invalid_arguments");

    let empty = parse(&request("chat.reply", json!({"text": "   "}))).expect_err("empty");
    assert_eq!(empty.code, "empty_message");

    let control = parse(&request("chat.reply", json!({"text": "a\u{7}"}))).expect_err("control");
    assert_eq!(control.code, "control_characters");

    let too_long = parse(&request(
        "chat.reply",
        json!({"text": "a".repeat(hn_core_chat::MAX_MESSAGE_BYTES + 1)}),
    ))
    .expect_err("too long");
    assert_eq!(too_long.code, "invalid_body");
    assert_eq!(too_long.field, Some("text"));

    let bad_role = parse(&request(
        "agent.spawn",
        json!({"name": "Alfred", "role": "alfred"}),
    ))
    .expect_err("role");
    assert_eq!(bad_role.field, Some("role"));

    let unknown =
        parse(&request("shell.exec", json!({"command": "rm"}))).expect_err("unknown tool");
    assert_eq!(unknown.code, "unknown_tool");
}

#[test]
fn tool_definitions_list_the_team_tools() {
    let names: Vec<String> = definitions()["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name").to_string())
        .collect();
    assert_eq!(
        names,
        [
            "chat.reply",
            "chat.ask_user",
            "chat.progress",
            "project.create",
            "agent.spawn",
            "delegation.assign",
            "plan.propose",
            "plan.status",
            "delegation.complete",
            "approval.request",
            "approval.decide",
            "approval.pending",
            "status.overview",
            "mail.accounts",
            "mail.draft_create",
            "mail.draft_update",
            "mail.draft_get",
            "mail.attach_file",
            "mail.inbox_list",
            "mail.message_get",
            "mail.triage",
            "mail.folders",
            "note.save",
            "note.search",
            "note.update",
            "note.delete",
            "contact.find",
            "contact.save",
            "contact.delete",
            "calendar.timezone",
            "calendar.list",
            "calendar.create",
            "calendar.update",
            "calendar.delete",
            "alert.rule_create",
            "alert.rule_list",
            "alert.rule_update",
            "alert.rule_delete",
            "alert.mode_set",
            "mail.move_propose",
            "mail.move_apply",
            "mail.undo_list",
            "mail.undo",
            "doc.create",
            "doc.read",
        ]
    );
}

/// `doc.read` parses to a typed read, and the host decides what the file is.
#[test]
fn doc_read_is_typed() {
    assert_eq!(
        parse(&request(
            "doc.read",
            json!({"attachment_id": "att_1", "sheet": "Hoja1", "page": 1})
        )),
        Ok(ToolCall::Doc(DocRead {
            attachment_id: "att_1".to_string(),
            sheet: Some("Hoja1".to_string()),
            range: None,
            page: Some(1),
        }))
    );
    let refused = parse(&request(
        "doc.read",
        json!({"attachment_id": "att_1", "page": 0}),
    ))
    .expect_err("page 0");
    assert_eq!(refused.code, "invalid_arguments");
}

/// The approval mode is the operator's (CORE9-D, RFC §8): no agent tool
/// lists it or writes it, and calling one that would is an unknown tool.
#[test]
fn no_agent_tool_changes_the_approval_mode() {
    let names = definitions()["tools"].to_string();
    assert!(!names.contains("approval.set_mode"), "{names}");
    assert!(!names.contains("settings"), "{names}");
    for tool in ["approval.set_mode", "approval.settings"] {
        let request = ToolRequest {
            tool: tool.to_string(),
            arguments: json!({ "mode": "always_user" }),
        };
        let refused = parse(&request).expect_err("no such tool");
        assert_eq!(refused.code, "unknown_tool", "{tool}");
    }
}

/// The alert tools are typed, and a rule is checked here before the store sees it (F5).
#[test]
fn alert_tools_are_typed_and_checked() {
    let rule = NewAlertRule::new("pane_status", Some("Bob"), Some("blocked"), "notify")
        .expect("a valid rule");
    assert_eq!(
        parse(&request(
            "alert.rule_create",
            json!({"trigger": "pane_status", "agent": "Bob", "status": "blocked", "action": "notify"})
        )),
        Ok(ToolCall::Alert(AlertCall::RuleCreate(rule)))
    );
    assert_eq!(
        parse(&request("alert.rule_list", json!({}))),
        Ok(ToolCall::Alert(AlertCall::RuleList))
    );
    assert_eq!(
        parse(&request(
            "alert.rule_update",
            json!({"id": "ar_1", "enabled": false})
        )),
        Ok(ToolCall::Alert(AlertCall::RuleUpdate {
            id: "ar_1".to_string(),
            enabled: false,
        }))
    );
    assert_eq!(
        parse(&request("alert.rule_delete", json!({"id": "ar_1"}))),
        Ok(ToolCall::Alert(AlertCall::RuleDelete {
            id: "ar_1".to_string(),
        }))
    );
    assert_eq!(
        parse(&request("alert.mode_set", json!({"mode": "rules_only"}))),
        Ok(ToolCall::Alert(AlertCall::ModeSet(AlertMode::RulesOnly)))
    );
}

#[test]
fn alert_tools_refuse_what_they_cannot_check() {
    let refused = [
        request(
            "alert.rule_create",
            json!({"trigger": "pane_status", "action": "notify"}),
        ),
        request(
            "alert.rule_create",
            json!({"trigger": "pane_status", "status": "done", "action": "notify", "extra": 1}),
        ),
        request(
            "alert.rule_create",
            json!({"trigger": "calendar_reminder", "status": "done", "action": "mute"}),
        ),
        request("alert.rule_list", json!({"all": true})),
        request("alert.rule_update", json!({"id": "ar_1", "enabled": "yes"})),
        request("alert.rule_delete", json!({"id": ""})),
        request("alert.mode_set", json!({"mode": "loud"})),
    ];
    for request in &refused {
        let error = parse(request).expect_err(&request.tool);
        assert_eq!(error.code, "invalid_arguments", "{}", request.tool);
    }
}

#[test]
fn a_delegation_without_steps_declares_one_step() {
    let parsed = parse(&request(
        "delegation.assign",
        json!({"to_agent_id": "ag_1", "task": "Plan", "plan_id": "plan_1"}),
    ));
    let Ok(ToolCall::DelegationAssign(assign)) = parsed else {
        panic!("assign: {parsed:?}")
    };
    assert_eq!(assign.steps, 1);
}
