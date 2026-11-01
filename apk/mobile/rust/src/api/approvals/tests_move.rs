//! `mail.move` batches in the approval rows, and the exclusions a decision sends.

use super::model::parse_approval_inbox;
use super::plan::plan_approval_decision;

const MOVE_ROW: &str = r#"{"ok":true,"approvals":[{"id":"apr_mv","status":"pending",
  "kind":"tool","tool":"mail.move","summary":"Mover 3 correos",
  "details":{"account":"trabajo","items":[
    {"message_id":"m1","dest":"junkemail","reason":"Phishing probable","from_address":"x@ejemplo.org",
     "subject":"<b>Gana</b>\nahora","level":"phish_candidate","excluded":false},
    {"message_id":"m2","dest":"inbox","reason":"Volver","subject":"Factura","excluded":true},
    {"message_id":"m3","dest":"hacked","reason":"nope"},
    {"dest":"deleteditems","reason":"sin id"}]}}]}"#;

#[test]
fn a_mail_move_row_keeps_the_valid_items_and_their_exclusions() {
    let rows = parse_approval_inbox(MOVE_ROW.to_string()).expect("inbox");
    let batch = rows[0].mail_move.clone().expect("move details");
    assert_eq!(batch.account, "trabajo");
    let ids: Vec<&str> = batch
        .items
        .iter()
        .map(|item| item.message_id.as_str())
        .collect();
    assert_eq!(ids, vec!["m1", "m2"]);
    assert_eq!(batch.items[0].dest, "junkemail");
    assert_eq!(batch.items[0].subject, "<b>Gana</b>\nahora");
    assert_eq!(
        batch.items[0].from_address.as_deref(),
        Some("x@ejemplo.org")
    );
    assert_eq!(batch.items[0].level.as_deref(), Some("phish_candidate"));
    assert!(!batch.items[0].excluded);
    assert!(batch.items[1].excluded);
    assert!(batch.items[1].from_address.is_none());
    assert!(rows[0].mail_send.is_none() && rows[0].mail_attach.is_none());
}

#[test]
fn a_mail_move_without_usable_items_has_no_details() {
    let json = r#"{"ok":true,"approvals":[
      {"id":"apr_1","status":"pending","tool":"mail.move","details":{"account":"trabajo"}},
      {"id":"apr_2","status":"pending","tool":"mail.move","details":{"account":"t","items":[{"dest":"inbox"}]}},
      {"id":"apr_3","status":"pending","tool":"mail.send","details":{"account":"t","items":[]}}]}"#;
    let rows = parse_approval_inbox(json.to_string()).expect("inbox");
    assert!(rows.iter().all(|row| row.mail_move.is_none()));
    assert!(rows[2].mail_send.is_some());
}

#[test]
fn a_decision_sends_exclude_only_when_there_is_one() {
    let call = plan_approval_decision(
        "apr_1".to_string(),
        true,
        "c-1".to_string(),
        vec!["m2".into()],
    )
    .request
    .expect("request");
    let body: serde_json::Value = serde_json::from_slice(&call.body).expect("json");
    assert_eq!(body["exclude"], serde_json::json!(["m2"]));

    let plain = plan_approval_decision("apr_1".to_string(), true, "c-1".to_string(), vec![])
        .request
        .expect("request");
    let body: serde_json::Value = serde_json::from_slice(&plain.body).expect("json");
    assert!(body.get("exclude").is_none());
}

#[test]
fn exclusions_follow_the_host_rules() {
    let ids = |count: usize, len: usize| -> Vec<String> {
        (0..count)
            .map(|n| format!("m{n}{}", "x".repeat(len)))
            .collect()
    };
    let refused = |approve: bool, exclude: Vec<String>| {
        plan_approval_decision("apr_1".to_string(), approve, "c-1".to_string(), exclude)
            .problem
            .is_some()
    };
    assert!(!refused(true, ids(20, 0)));
    assert!(refused(true, ids(21, 0)), "21 ids");
    assert!(
        refused(false, vec!["m1".to_string()]),
        "deny with exclusions"
    );
    assert!(refused(true, vec![String::new()]), "empty id");
    assert!(!refused(true, vec!["x".repeat(512)]));
    assert!(refused(true, vec!["x".repeat(513)]), "id too long");
}

#[test]
fn a_frame_that_lands_first_keeps_the_tool_of_a_move_batch() {
    // Regression: the card had its checkboxes but no `tool`, so a decision sent no
    // `exclude` and the batch moved whole.
    let mut reducer = super::reducer::new_approval_reducer();
    let frame = crate::api::chat::model::parse_chat_event(
        r#"{"event":"chat.approval","revision":10,"conversation_id":null,"approval_id":"apr_mv",
           "level":"user","status":"pending","summary":"Mover 3 correos"}"#
            .to_string(),
    )
    .expect("chat event");
    assert!(reducer.accept_event(frame));
    let row = parse_approval_inbox(MOVE_ROW.to_string()).expect("inbox");
    assert!(reducer.accept_record(row[0].clone()));
    let view = reducer.user_inbox_unassigned();
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].tool.as_deref(), Some("mail.move"));
    assert_eq!(view[0].kind.as_deref(), Some("tool"));
    assert!(view[0].mail_move.is_some());
}
