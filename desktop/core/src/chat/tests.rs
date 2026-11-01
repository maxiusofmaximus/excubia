//! Chat contract tests: error mapping, patch shape and identifier bounds.

use reqwest::StatusCode;
use serde_json::Value;

use super::error::status_failure;
use super::*;
use crate::error::CoreError;

fn status(status: u16, code: &str, message: &str) -> CoreError {
    CoreError::Status {
        status,
        code: Some(code.to_string()),
        field: None,
        message: message.to_string(),
    }
}

#[test]
fn a_missing_respond_is_the_forbidden_card_with_the_grant_command() {
    let failure = ChatFailure::from_core(&CoreError::Forbidden, true);
    match failure {
        ChatFailure::Forbidden { message, action } => {
            assert!(message.contains("RESPOND"));
            assert!(action.contains("host devices grant"));
        }
        other => panic!("esperaba forbidden, llegó {other:?}"),
    }
}

#[test]
fn the_two_409s_of_the_seat_are_distinct() {
    let seat = ChatFailure::from_core(&status(409, "seat_unavailable", "seat"), true);
    assert!(matches!(seat, ChatFailure::SeatUnavailable { .. }));

    let busy = ChatFailure::from_core(&status(409, "agent_busy", "busy"), true);
    match busy {
        ChatFailure::AgentBusy { status, .. } => assert_eq!(status, 409),
        other => panic!("esperaba agent_busy, llegó {other:?}"),
    }

    let archived = ChatFailure::from_core(&status(409, "conversation_archived", "x"), true);
    assert!(matches!(archived, ChatFailure::Archived { .. }));
}

#[test]
fn a_422_names_the_field_and_a_404_is_not_found() {
    let invalid = ChatFailure::from_core(
        &CoreError::Status {
            status: 422,
            code: Some("invalid_body".to_string()),
            field: Some("text".to_string()),
            message: "invalid".to_string(),
        },
        true,
    );
    match invalid {
        ChatFailure::Invalid { field, .. } => assert_eq!(field.as_deref(), Some("text")),
        other => panic!("esperaba invalid, llegó {other:?}"),
    }

    let missing = ChatFailure::from_core(&status(404, "no_such_conversation", "x"), false);
    assert!(matches!(missing, ChatFailure::NotFound { .. }));
}

#[test]
fn ids_are_bounded_and_prefix_checked() {
    assert!(validate_prefixed_id("cnv_01JABC", "cnv_", "conversación").is_ok());
    assert!(validate_prefixed_id("cnv_", "cnv_", "conversación").is_err());
    assert!(validate_prefixed_id("cnv_a/b", "cnv_", "conversación").is_err());
    assert!(validate_client_id("6d0f-88bc").is_ok());
    assert!(validate_client_id("").is_err());
    assert!(validate_client_id("bad\nid").is_err());
}

#[test]
fn a_429_keeps_its_retry_after() {
    let body = br#"{"detail":"agent_busy"}"#;
    let failure = status_failure(StatusCode::TOO_MANY_REQUESTS, body, Some(18), true);
    match failure {
        ChatFailure::AgentBusy {
            retry_after_secs,
            status,
            ..
        } => {
            assert_eq!(retry_after_secs, Some(18));
            assert_eq!(status, 429);
        }
        other => panic!("esperaba agent_busy, llegó {other:?}"),
    }
}

#[test]
fn a_team_invalid_name_stays_a_team_refusal() {
    let body = br#"{"detail":{"code":"invalid_name","field":"name","message":"x"}}"#;
    let failure = status_failure(StatusCode::UNPROCESSABLE_ENTITY, body, None, true);
    assert!(matches!(failure, ChatFailure::TeamRefused { .. }));
}

#[test]
fn an_activity_frame_keeps_the_delegation_and_its_status() {
    let event = ChatEvent::from_frame(
        "chat.activity",
        &serde_json::json!({
            "revision": 58,
            "conversation_id": "cnv_1",
            "delegation_id": "dlg_1",
            "card_id": "act_dlg_1",
            "pane_id": "w1:p3",
            "status": "waiting",
            "summary": "necesita permiso"
        }),
    );
    assert_eq!(event.event, "chat.activity");
    assert_eq!(event.delegation_id.as_deref(), Some("dlg_1"));
    assert_eq!(event.card_id.as_deref(), Some("act_dlg_1"));
    assert_eq!(event.status.as_deref(), Some("waiting"));
    assert_eq!(event.pane_id.as_deref(), Some("w1:p3"));
}

#[test]
fn the_delegation_rules_are_typed_and_in_spanish() {
    for code in [
        "depth_exceeded",
        "pane_limit",
        "cycle_detected",
        "turn_budget",
        "hour_budget",
        "delegation_cancelled",
    ] {
        let failure = ChatFailure::from_core(&status(409, code, code), true);
        match failure {
            ChatFailure::DelegationRefused {
                code: got,
                message,
                status,
            } => {
                assert_eq!(got, code);
                assert_eq!(status, 409);
                assert!(message.len() > 20, "{code}: {message}");
                assert!(!message.contains("El host rechazó"), "{code}: {message}");
            }
            other => panic!("esperaba DelegationRefused para {code}, llegó {other:?}"),
        }
    }

    let unknown = status_failure(
        StatusCode::NOT_FOUND,
        br#"{"detail":"no_such_delegation"}"#,
        None,
        false,
    );
    match unknown {
        ChatFailure::NotFound { message } => assert!(message.contains("delegación")),
        other => panic!("esperaba NotFound, llegó {other:?}"),
    }
}

#[test]
fn clearing_a_section_is_an_explicit_null() {
    let clear = ConversationPatch {
        section: SectionChange::Clear,
        ..ConversationPatch::default()
    };
    let body: Value = serde_json::from_slice(&clear.to_body().unwrap()).unwrap();
    assert!(body.get("section_id").unwrap().is_null());

    let keep = ConversationPatch::default();
    let body: Value = serde_json::from_slice(&keep.to_body().unwrap()).unwrap();
    assert!(body.get("section_id").is_none());
}

#[test]
fn turn_state_and_active_runs_are_optional_for_an_older_host() {
    let old: Message = serde_json::from_value(serde_json::json!({
        "id": "msg_1", "conversation_id": "cnv_1", "seq": 3,
        "author_kind": "user", "body_md": "hola", "state": "complete"
    }))
    .unwrap();
    assert!(old.turn_state.is_none());

    let new: Message = serde_json::from_value(serde_json::json!({
        "id": "msg_2", "conversation_id": "cnv_1", "seq": 4,
        "author_kind": "user", "body_md": "hola", "state": "complete",
        "turn_state": "running"
    }))
    .unwrap();
    assert_eq!(new.turn_state.as_deref(), Some("running"));

    let conversation: Conversation = serde_json::from_value(serde_json::json!({
        "id": "cnv_1", "title": "Plan"
    }))
    .unwrap();
    assert_eq!(conversation.active_runs, 0);
}

#[test]
fn a_turn_frame_carries_its_state_and_no_message_state() {
    let event = ChatEvent::from_frame(
        "chat.message",
        &serde_json::json!({
            "revision": 71,
            "conversation_id": "cnv_1",
            "message_id": "msg_2",
            "change": "updated",
            "author_kind": "user",
            "turn_state": "interrupted"
        }),
    );
    assert_eq!(event.turn_state.as_deref(), Some("interrupted"));
    assert_eq!(event.message_id.as_deref(), Some("msg_2"));
    assert!(event.state.is_none());
}

#[test]
fn suggestions_quote_and_the_filter_flags_are_optional_on_the_wire() {
    let plain: TimelineEntry =
        serde_json::from_value(serde_json::json!({"seq": 1, "kind": "message"})).unwrap();
    assert!(plain.suggestions.is_empty());
    assert!(plain.quote.is_none());

    let rich: TimelineEntry = serde_json::from_value(serde_json::json!({
        "seq": 2, "kind": "message", "suggestions": ["Sí, créalo"],
        "quote": {"kind": "message", "id": "msg_1", "author_kind": "agent", "preview": "¿Qué nombre?"}
    }))
    .unwrap();
    assert_eq!(rich.suggestions, ["Sí, créalo"]);
    assert_eq!(
        rich.quote.map(|quote| quote.preview).as_deref(),
        Some("¿Qué nombre?")
    );

    assert_eq!(ListFilter::parse("working"), Some(ListFilter::Working));
    assert_eq!(ListFilter::parse("0"), None);
}

#[test]
fn the_progress_line_is_optional_on_the_conversation_and_absent_when_unset() {
    let plain: Conversation =
        serde_json::from_value(serde_json::json!({"id": "cnv_1", "title": "Plan"})).unwrap();
    assert!(plain.progress.is_none());
    let encoded = serde_json::to_value(&plain).unwrap();
    assert!(encoded.get("progress").is_none());

    let working: Conversation = serde_json::from_value(serde_json::json!({
        "id": "cnv_1", "title": "Plan", "active_runs": 1,
        "progress": "Revisando los borradores"
    }))
    .unwrap();
    assert_eq!(
        working.progress.as_deref(),
        Some("Revisando los borradores")
    );
}

#[test]
fn a_progress_frame_keeps_its_run_and_line_and_a_close_has_no_line() {
    let line = ChatEvent::from_frame(
        "chat.progress",
        &serde_json::json!({
            "revision": 62, "conversation_id": "cnv_1",
            "run_id": "run_1", "text": "Revisando los borradores"
        }),
    );
    assert_eq!(line.event, "chat.progress");
    assert_eq!(line.run_id.as_deref(), Some("run_1"));
    assert_eq!(line.text.as_deref(), Some("Revisando los borradores"));

    let close = ChatEvent::from_frame(
        "chat.progress",
        &serde_json::json!({
            "revision": 63, "conversation_id": "cnv_1",
            "run_id": "run_1", "text": null
        }),
    );
    assert!(close.text.is_none());
    assert_eq!(close.run_id.as_deref(), Some("run_1"));
}
