//! The `host.log` lines of what an approval card and a delegation did, one line per change.
//! A line names the kind, the seat and a short id. It never carries a command, a URL, a token
//! or a code: the frame's `summary` is left out on purpose.

use serde_json::Value;

use crate::model::ChatFrame;

/// The `host.log` line of a published frame, `None` when the log does not follow it.
pub(super) fn host_line(frame: &ChatFrame) -> Option<String> {
    let body = &frame.body;
    match frame.event.as_str() {
        "chat.approval" => approval_line(body),
        "chat.delegation" if field(body, "status") == "queued" => Some(format!(
            "delegation sent: to={} id={}",
            field(body, "to_agent_id"),
            short(field(body, "delegation_id")),
        )),
        "chat.activity" => {
            let status = field(body, "status");
            let id = field(body, "delegation_id");
            match status {
                "done" | "failed" if !id.is_empty() => {
                    Some(format!("delegation {status}: id={}", short(id)))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn approval_line(body: &Value) -> Option<String> {
    let verb = match (field(body, "change"), field(body, "status")) {
        ("created", _) => "created",
        ("decided", "approved") => "allowed",
        ("decided", "denied") => "denied",
        ("expired", _) => "expired",
        _ => return None,
    };
    let by = match verb {
        "allowed" | "denied" => decider(body).map_or_else(String::new, |by| format!(" by={by}")),
        _ => String::new(),
    };
    Some(format!(
        "approval {verb}: kind={} agent={} id={}{by}",
        field(body, "kind"),
        field(body, "agent_id"),
        short(field(body, "approval_id")),
    ))
}

/// Who decided, as the frame names it. A rule, an agent or the scorer is named; the
/// operator's device is only `operator`, its id never reaches the log.
fn decider(body: &Value) -> Option<String> {
    if field(body, "auto") == "task-scope" {
        return Some("task-scope".to_string());
    }
    let kind = field(body, "decider_kind");
    let id = field(body, "decider_id");
    match kind {
        "" => None,
        "user" => Some("operator".to_string()),
        "policy" | "agent" if !id.is_empty() => Some(format!("{kind}:{id}")),
        _ => Some(kind.to_string()),
    }
}

fn field<'a>(body: &'a Value, key: &str) -> &'a str {
    body[key].as_str().unwrap_or_default()
}

/// The last eight characters of an id: the random part of a ULID, enough to tell two apart.
fn short(id: &str) -> &str {
    id.char_indices()
        .rev()
        .nth(7)
        .map_or(id, |(index, _)| &id[index..])
}
