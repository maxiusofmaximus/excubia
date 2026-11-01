use super::error::{approval_grant_command, explain_approval_failure, ApprovalFailureKind};
use super::model::{
    parse_approval_inbox, parse_approval_record, parse_approval_settings, MailAttachDetails,
};
use super::plan::{plan_approval_decision, plan_approval_inbox, plan_approval_settings_write};
use super::reducer::new_approval_reducer;
use crate::api::chat::model::parse_chat_event;
use crate::api::stream::new_stream_decoder;

const INBOX: &str = r#"{"ok":true,"approvals":[{"id":"apr_1","conversation_id":"cnv_1",
  "seat_id":"seat_ernesto","pane_id":"w1:p9","kind":"command","tool":"bash",
  "summary":"Ejecutar comando: cargo test","cwd":"demo/api","risk":"medium",
  "status":"pending","level":"user","expires_at":"2026-10-22T10:12:00.000Z",
  "payload_redacted":{"command":"cargo test"}}]}"#;

fn frame(json: &str) -> crate::api::chat::model::ChatEvent {
    parse_chat_event(json.to_string()).expect("chat event")
}

#[test]
fn inbox_rows_map_id_and_level() {
    let rows = parse_approval_inbox(INBOX.to_string()).expect("inbox");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].approval_id, "apr_1");
    assert_eq!(rows[0].summary, "Ejecutar comando: cargo test");
    assert_eq!(rows[0].cwd.as_deref(), Some("demo/api"));
    assert_eq!(rows[0].level.as_deref(), Some("user"));
    assert_eq!(rows[0].status, "pending");
}

#[test]
fn inbox_skips_rows_with_unknown_status() {
    let json = r#"{"ok":true,"approvals":[{"id":"apr_x","status":"weird"},{"id":"apr_2","status":"expired"}]}"#;
    let rows = parse_approval_inbox(json.to_string()).expect("inbox");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].approval_id, "apr_2");
}

#[test]
fn decision_answer_nests_the_record_and_keeps_the_audit() {
    let json = r#"{"ok":true,"approval":{"id":"apr_1","status":"approved","decided_by":"dev_1",
      "decided_at":"2026-10-22T10:11:00.000Z","level":"lead",
      "audit":[{"device_id":"dev_1","decision":"approve","decided_at":"2026-10-22T10:11:00.000Z"}]}}"#;
    let record = parse_approval_record(json.to_string()).expect("record");
    assert_eq!(record.status, "approved");
    assert_eq!(record.decider_id.as_deref(), Some("dev_1"));
    assert_eq!(record.audit.len(), 1);
    assert_eq!(record.audit[0].decision, "approve");
}

#[test]
fn state_read_is_a_flat_record_without_summary() {
    let json = r#"{"ok":true,"approval_id":"apr_1","status":"pending","expires_at":"2026-10-22T10:12:00.000Z"}"#;
    let record = parse_approval_record(json.to_string()).expect("record");
    assert_eq!(record.approval_id, "apr_1");
    assert_eq!(record.summary, "");
    assert_eq!(
        record.expires_at.as_deref(),
        Some("2026-10-22T10:12:00.000Z")
    );
}

const MAIL_SEND_ROW: &str = r#"{"ok":true,"approvals":[{"id":"apr_m1","status":"pending",
  "kind":"tool","tool":"mail.send","summary":"Enviar correo a bea@ejemplo.org",
  "expires_at":"2026-10-25T10:12:00.000Z",
  "details":{"account":"trabajo","from":"ana@ejemplo.com","to":["bea@ejemplo.org"],
    "cc":["carlos@ejemplo.org"],"subject":"Presupuesto","body_preview":"Hola Bea…",
    "attachments":[{"name":"presupuesto.pdf","size":12034,"sha256":"ab12"}],
    "device_code":"dc_secret","access_token":"tok_secret"}}]}"#;

#[test]
fn a_mail_send_row_carries_its_details() {
    let rows = parse_approval_inbox(MAIL_SEND_ROW.to_string()).expect("inbox");
    let send = rows[0].mail_send.clone().expect("send details");
    assert_eq!(send.account, "trabajo");
    assert_eq!(send.from.as_deref(), Some("ana@ejemplo.com"));
    assert_eq!(send.to, vec!["bea@ejemplo.org".to_string()]);
    assert_eq!(send.cc, vec!["carlos@ejemplo.org".to_string()]);
    assert_eq!(send.subject, "Presupuesto");
    assert_eq!(send.body_preview, "Hola Bea…");
    assert_eq!(send.attachments.len(), 1);
    assert_eq!(send.attachments[0].name, "presupuesto.pdf");
    assert_eq!(send.attachments[0].size, 12034);
    assert_eq!(send.attachments[0].sha256, "ab12");
    assert!(rows[0].mail_attach.is_none());
}

#[test]
fn a_mail_send_details_never_hold_a_token_or_a_device_code() {
    let rows = parse_approval_inbox(MAIL_SEND_ROW.to_string()).expect("inbox");
    let debug = format!("{:?}", rows[0]);
    assert!(!debug.contains("dc_secret"));
    assert!(!debug.contains("tok_secret"));
}

#[test]
fn an_attach_path_row_carries_path_size_and_digest() {
    let json = r#"{"ok":true,"approvals":[{"id":"apr_a1","status":"pending",
      "kind":"tool","tool":"mail.attach_path","summary":"Adjuntar un fichero",
      "details":{"path":"D:/fuera/contrato.pdf","size":512,"sha256":"cd34"}}]}"#;
    let rows = parse_approval_inbox(json.to_string()).expect("inbox");
    assert!(rows[0].mail_send.is_none());
    assert_eq!(
        rows[0].mail_attach,
        Some(MailAttachDetails {
            path: "D:/fuera/contrato.pdf".to_string(),
            size: 512,
            sha256: "cd34".to_string(),
        })
    );
}

#[test]
fn an_approval_without_details_or_with_unknown_details_has_none() {
    let json = r#"{"ok":true,"approvals":[
      {"id":"apr_1","status":"pending","summary":"Ejecutar comando"},
      {"id":"apr_2","status":"pending","summary":"Otra","details":{"cosa_nueva":1}}]}"#;
    let rows = parse_approval_inbox(json.to_string()).expect("inbox");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].mail_send.is_none() && rows[0].mail_attach.is_none());
    assert!(rows[1].mail_send.is_none() && rows[1].mail_attach.is_none());
}

#[test]
fn settings_keep_only_valid_modes() {
    let json = r#"{"ok":true,"settings":{"global":"always_user",
      "projects":[{"project_id":"prj_a","mode":"delegated"},{"project_id":"prj_b","mode":"nope"}]},
      "replayed":false,"escalated":2}"#;
    let settings = parse_approval_settings(json.to_string()).expect("settings");
    assert_eq!(settings.global, "always_user");
    assert_eq!(settings.projects.len(), 1);
    assert_eq!(settings.projects[0].project_id, "prj_a");
    assert_eq!(settings.escalated, 2);
    let invalid = r#"{"ok":true,"settings":{"global":"allow_all","projects":[]}}"#;
    assert!(parse_approval_settings(invalid.to_string()).is_none());
}

#[test]
fn reducer_moves_forward_and_never_back() {
    let mut reducer = new_approval_reducer();
    let pending = frame(
        r#"{"event":"chat.approval","revision":10,"conversation_id":"cnv_1","approval_id":"apr_1",
           "level":"user","escalated_from":"lead","status":"pending","change":"escalated",
           "summary":"Ejecutar comando: cargo test","expires_at":"2026-10-22T10:12:00.000Z"}"#,
    );
    assert!(reducer.accept_event(pending));
    let decided = frame(
        r#"{"event":"chat.approval","revision":12,"conversation_id":"cnv_1","approval_id":"apr_1",
           "status":"approved","change":"decided"}"#,
    );
    assert!(reducer.accept_event(decided));
    // A late pending frame must not reopen the card.
    let late = frame(
        r#"{"event":"chat.approval","revision":11,"conversation_id":"cnv_1","approval_id":"apr_1",
           "status":"pending","change":"escalated"}"#,
    );
    assert!(!reducer.accept_event(late));
    let views = reducer.snapshot(Some("cnv_1".to_string()));
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].status, "approved");
    // A frame without a summary keeps the one already stored.
    assert_eq!(views[0].summary, "Ejecutar comando: cargo test");
    assert_eq!(views[0].level.as_deref(), Some("user"));
}

#[test]
fn a_frame_that_lands_first_keeps_the_mail_details_of_the_inbox() {
    // Regression: «Falta el detalle del comando» on a mail.send card. The frame
    // arrives before the inbox read, which has no revision and so looks older.
    let mut reducer = new_approval_reducer();
    let pending = frame(
        r#"{"event":"chat.approval","revision":10,"conversation_id":null,"approval_id":"apr_m1",
           "level":"user","status":"pending","summary":"Enviar correo a bea@ejemplo.org"}"#,
    );
    assert!(reducer.accept_event(pending));
    let row = parse_approval_inbox(MAIL_SEND_ROW.to_string()).expect("inbox");
    assert!(reducer.accept_record(row[0].clone()));
    let view = reducer.user_inbox_unassigned();
    assert_eq!(view.len(), 1);
    let send = view[0].mail_send.clone().expect("send details kept");
    assert_eq!(send.subject, "Presupuesto");
    assert_eq!(send.account, "trabajo");
    // A newer frame of the same pending state keeps those facts too.
    let newer = frame(
        r#"{"event":"chat.approval","revision":11,"conversation_id":null,"approval_id":"apr_m1",
           "level":"user","status":"pending"}"#,
    );
    reducer.accept_event(newer);
    assert_eq!(
        reducer.user_inbox_unassigned()[0]
            .mail_send
            .as_ref()
            .map(|send| send.subject.as_str()),
        Some("Presupuesto")
    );
}

#[test]
fn reducer_ignores_other_chat_events_and_unknown_status() {
    let mut reducer = new_approval_reducer();
    let message =
        frame(r#"{"event":"chat.message","revision":3,"conversation_id":"cnv_1","seq":4}"#);
    assert!(!reducer.accept_event(message));
    let weird = frame(
        r#"{"event":"chat.approval","revision":4,"conversation_id":"cnv_1","approval_id":"apr_9","status":"maybe"}"#,
    );
    assert!(!reducer.accept_event(weird));
    assert!(reducer.snapshot(None).is_empty());
}

#[test]
fn user_pending_lists_only_pending_user_level_or_unknown() {
    let mut reducer = new_approval_reducer();
    for record in [
        r#"{"id":"apr_user","status":"pending","level":"user","conversation_id":"cnv_1"}"#,
        r#"{"id":"apr_lead","status":"pending","level":"lead","conversation_id":"cnv_1"}"#,
        r#"{"id":"apr_done","status":"approved","level":"user","conversation_id":"cnv_1"}"#,
        r#"{"id":"apr_other","status":"pending","level":"user","conversation_id":"cnv_2"}"#,
    ] {
        let value =
            parse_approval_record(format!(r#"{{"ok":true,"approval":{record}}}"#)).expect("record");
        reducer.accept_record(value);
    }
    let pending: Vec<String> = reducer
        .user_pending(Some("cnv_1".to_string()))
        .into_iter()
        .map(|approval| approval.approval_id)
        .collect();
    assert_eq!(pending, vec!["apr_user".to_string()]);
}

#[test]
fn frame_without_conversation_reaches_the_inbox_only() {
    let mut reducer = new_approval_reducer();
    let unassigned = frame(
        r#"{"event":"chat.approval","revision":7,"conversation_id":null,"approval_id":"apr_seat",
        "status":"pending","level":"user","summary":"Ejecutar comando: echo x"}"#,
    );
    assert!(reducer.accept_event(unassigned));
    let inbox: Vec<String> = reducer
        .user_inbox_unassigned()
        .into_iter()
        .map(|approval| approval.approval_id)
        .collect();
    assert_eq!(inbox, vec!["apr_seat".to_string()]);
    assert!(reducer.user_pending(Some("cnv_1".to_string())).is_empty());
    let denied = frame(
        r#"{"event":"chat.approval","revision":8,"conversation_id":null,"approval_id":"apr_seat",
        "status":"denied"}"#,
    );
    assert!(reducer.accept_event(denied));
    let closed = reducer.user_inbox_unassigned();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].status, "denied");
    assert!(reducer.user_pending(None).is_empty());
}

#[test]
fn unassigned_inbox_skips_approvals_that_belong_to_a_conversation() {
    let mut reducer = new_approval_reducer();
    reducer.accept_record(
        parse_approval_record(
            r#"{"ok":true,"approval":{"id":"apr_conv","status":"pending","level":"user","conversation_id":"cnv_1"}}"#
                .to_string(),
        )
        .expect("record"),
    );
    assert!(reducer.user_inbox_unassigned().is_empty());
}

#[test]
fn plans_build_routes_bodies_and_refuse_bad_input() {
    let inbox = plan_approval_inbox(Some("cnv_1".to_string()), Some("expired".to_string()));
    let call = inbox.request.expect("inbox request");
    assert_eq!(call.method, "GET");
    assert_eq!(
        call.target,
        "/v1/chat/approvals?status=expired&conversation_id=cnv_1"
    );

    let decision = plan_approval_decision("apr_1".to_string(), true, "c-1".to_string(), vec![]);
    let call = decision.request.expect("decision request");
    assert_eq!(call.method, "POST");
    assert_eq!(call.target, "/v1/chat/approvals/apr_1/decision");
    let body: serde_json::Value = serde_json::from_slice(&call.body).expect("json");
    assert_eq!(body["decision"], "approve");
    assert_eq!(body["scope"], "once");
    assert_eq!(body["client_id"], "c-1");

    let deny = plan_approval_decision("apr_1".to_string(), false, "c-2".to_string(), vec![]);
    let body: serde_json::Value =
        serde_json::from_slice(&deny.request.expect("deny").body).expect("json");
    assert_eq!(body["decision"], "deny");

    assert!(
        plan_approval_decision("../x".to_string(), true, "c-1".to_string(), vec![])
            .problem
            .is_some()
    );
    assert!(plan_approval_inbox(None, Some("maybe".to_string()))
        .problem
        .is_some());
    assert!(
        plan_approval_settings_write("allow_all".to_string(), None, "c-3".to_string())
            .problem
            .is_some()
    );
}

#[test]
fn settings_write_carries_project_only_when_given() {
    let global = plan_approval_settings_write("always_user".to_string(), None, "c-4".to_string());
    let body: serde_json::Value =
        serde_json::from_slice(&global.request.expect("global").body).expect("json");
    assert_eq!(body["mode"], "always_user");
    assert!(body.get("project_id").is_none());

    let project = plan_approval_settings_write(
        "delegated".to_string(),
        Some("prj_a".to_string()),
        "c-5".to_string(),
    );
    let body: serde_json::Value =
        serde_json::from_slice(&project.request.expect("project").body).expect("json");
    assert_eq!(body["project_id"], "prj_a");
}

#[test]
fn refusals_map_to_their_screen_state() {
    let forbidden = explain_approval_failure(
        403,
        r#"{"detail":"forbidden"}"#.to_string(),
        "dev_42".to_string(),
    );
    assert_eq!(forbidden.kind, ApprovalFailureKind::NeedsGrant);
    assert_eq!(
        forbidden.command.as_deref(),
        Some("host devices grant dev_42 --permission approve")
    );
    let decided = explain_approval_failure(
        409,
        r#"{"detail":"approval_already_decided"}"#.to_string(),
        String::new(),
    );
    assert_eq!(decided.kind, ApprovalFailureKind::AlreadyDecided);
    assert_eq!(decided.command, None);
    let expired = explain_approval_failure(
        409,
        r#"{"detail":{"code":"approval_expired"}}"#.to_string(),
        String::new(),
    );
    assert_eq!(expired.kind, ApprovalFailureKind::Expired);
    let invalid = explain_approval_failure(
        422,
        r#"{"detail":{"code":"invalid_mode"}}"#.to_string(),
        String::new(),
    );
    assert_eq!(invalid.kind, ApprovalFailureKind::Other);
    assert_eq!(invalid.code, "invalid_mode");
}

#[test]
fn grant_command_falls_back_to_a_placeholder_without_device() {
    assert_eq!(
        approval_grant_command("  ".to_string()),
        "host devices grant <device_id> --permission approve"
    );
}

/// Frame capturado del host real (`demo-client events`), ids anonimizados:
/// `chat.approval` con `conversation_id` nulo que el SSE entrega a la bandeja.
#[test]
fn host_sse_frame_reaches_the_inbox_through_the_decoder() {
    let lines = [
        "id: 2",
        "event: chat.approval",
        r#"data: {"agent_id":"ag_ANON","approval_id":"apr_ANON","change":"created","conversation_id":null,"decider_id":null,"decider_kind":null,"escalated_from":"policy","event":"chat.approval","expires_at":"2026-10-23T18:39:33.579Z","kind":"command","level":"user","pane_id":null,"revision":2,"risk_score":null,"scorer_version":null,"status":"pending","summary":"command Bash"}"#,
        "",
    ];
    let mut decoder = new_stream_decoder();
    let mut reducer = new_approval_reducer();
    let mut changed = false;
    for line in lines {
        if let Some(chat) = decoder
            .push_line(line.to_string())
            .and_then(|event| event.chat)
        {
            changed = reducer.accept_event(chat);
        }
    }
    assert!(changed, "the decoder must yield the chat.approval frame");
    let inbox: Vec<String> = reducer
        .user_inbox_unassigned()
        .into_iter()
        .map(|approval| approval.approval_id)
        .collect();
    assert_eq!(inbox, vec!["apr_ANON".to_string()]);
}
