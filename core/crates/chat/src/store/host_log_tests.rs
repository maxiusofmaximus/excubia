use serde_json::{json, Value};

use super::host_log::host_line;
use crate::model::ChatFrame;

/// What must never reach the log: the command, a token, a URL with a query and a code.
const SECRETS: [&str; 4] = [
    "rm -rf C:/SECRET_PATH",
    "TOKEN_SENTINEL_123",
    "https://example.test/?code=CODE_SENTINEL",
    "SENTINEL",
];

fn frame(event: &str, body: Value) -> ChatFrame {
    ChatFrame {
        id: 1,
        event: event.to_string(),
        body,
    }
}

fn line_of(event: &str, body: Value) -> String {
    let line = host_line(&frame(event, body)).expect("this frame has a line");
    for secret in SECRETS {
        assert!(!line.contains(secret), "the line leaks {secret}: {line}");
    }
    line
}

#[test]
fn approval_created_names_kind_seat_and_short_id() {
    let line = line_of(
        "chat.approval",
        json!({
            "change": "created", "status": "pending", "kind": "command",
            "agent_id": "alfred", "approval_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(
        line,
        "approval created: kind=command agent=alfred id=E5F6G7H8"
    );
}

#[test]
fn approval_allowed_when_decided_approved() {
    let line = line_of(
        "chat.approval",
        json!({
            "change": "decided", "status": "approved", "kind": "network",
            "agent_id": "alfred", "approval_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(
        line,
        "approval allowed: kind=network agent=alfred id=E5F6G7H8"
    );
}

#[test]
fn approval_denied_when_decided_denied() {
    let line = line_of(
        "chat.approval",
        json!({
            "change": "decided", "status": "denied", "kind": "tool",
            "agent_id": "lead", "approval_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(line, "approval denied: kind=tool agent=lead id=E5F6G7H8");
}

#[test]
fn approval_expired_line() {
    let line = line_of(
        "chat.approval",
        json!({
            "change": "expired", "status": "expired", "kind": "edit",
            "agent_id": "alfred", "approval_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(line, "approval expired: kind=edit agent=alfred id=E5F6G7H8");
}

#[test]
fn delegation_sent_to_the_lead_line() {
    let line = line_of(
        "chat.delegation",
        json!({
            "status": "queued", "to_agent_id": "lead",
            "delegation_id": "A1B2C3D4E5F6G7H8", "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(line, "delegation sent: to=lead id=E5F6G7H8");
}

#[test]
fn delegation_done_line() {
    let line = line_of(
        "chat.activity",
        json!({
            "status": "done", "delegation_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(line, "delegation done: id=E5F6G7H8");
}

#[test]
fn delegation_failed_line() {
    let line = line_of(
        "chat.activity",
        json!({
            "status": "failed", "delegation_id": "A1B2C3D4E5F6G7H8",
            "summary": SECRETS.join(" "),
        }),
    );
    assert_eq!(line, "delegation failed: id=E5F6G7H8");
}

#[test]
fn other_frames_give_no_line() {
    let silent = [
        ("chat.message", json!({"change": "created"})),
        (
            "chat.approval",
            json!({"change": "shown", "status": "pending"}),
        ),
        (
            "chat.approval",
            json!({"change": "escalated", "status": "pending"}),
        ),
        (
            "chat.delegation",
            json!({"status": "running", "delegation_id": "X"}),
        ),
        (
            "chat.activity",
            json!({"status": "running", "delegation_id": "X"}),
        ),
        (
            "chat.activity",
            json!({"status": "done", "delegation_id": ""}),
        ),
    ];
    for (event, body) in silent {
        assert!(
            host_line(&frame(event, body)).is_none(),
            "{event} has no line"
        );
    }
}
