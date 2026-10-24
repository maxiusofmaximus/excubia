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
