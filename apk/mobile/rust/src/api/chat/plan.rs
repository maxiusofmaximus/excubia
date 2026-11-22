//! `/v1/chat` request plans: route, query, body and local validation in one
//! place. Dart only signs and sends the bytes.

use serde_json::json;

use crate::api::error::{action_problem, ErrorInfo, FailureKind};
use crate::api::validate;
use crate::api::write::HttpCall;

use super::attach_plan::plan_chat_send_with_attachments;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPlan {
    pub request: Option<HttpCall>,
    pub problem: Option<ErrorInfo>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_bootstrap() -> ChatPlan {
    get("/v1/chat/bootstrap".to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_conversations(
    query: Option<String>,
    section_id: Option<String>,
    pinned: Option<bool>,
    archived: Option<bool>,
    attention: bool,
    working: bool,
    cursor: Option<String>,
) -> ChatPlan {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(query) = query.map(|query| query.trim().to_string()) {
        if !query.is_empty() {
            params.push(("q", encode(&query)));
        }
    }
    if let Some(section_id) = section_id {
        if !validate::identifier(&section_id) {
            return problem("bad_section");
        }
        params.push(("section_id", encode(&section_id)));
    }
    if let Some(pinned) = pinned {
        params.push(("pinned", pinned.to_string()));
    }
    if let Some(archived) = archived {
        params.push(("archived", archived.to_string()));
    }
    // The host answers 422 to `0` and `false`, so a filter that is off is omitted.
    if attention {
        params.push(("attention", "1".to_string()));
    }
    if working {
        params.push(("working", "1".to_string()));
    }
    if let Some(cursor) = cursor {
        if !validate::identifier(&cursor) {
            return problem("invalid_cursor");
        }
        params.push(("cursor", encode(&cursor)));
    }
    params.push(("limit", "100".to_string()));
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    get(format!("/v1/chat/conversations?{query}"))
}

/// The chip numbers in one aggregate. A host before this route answers 404 or 405.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_counts() -> ChatPlan {
    get("/v1/chat/conversations/counts".to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_create_conversation(title: Option<String>) -> ChatPlan {
    let mut body = serde_json::Map::new();
    match title {
        Some(title) => match clean_title(&title) {
            Ok(title) => {
                body.insert("title".to_string(), json!(title));
            }
            Err(code) => return problem(code),
        },
        None => {}
    }
    post("/v1/chat/conversations".to_string(), body)
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_rename_conversation(conversation_id: String, title: String) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    let title = match clean_title(&title) {
        Ok(title) => title,
        Err(code) => return problem(code),
    };
    update(conversation_id, json!({ "title": title }))
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_pin_conversation(conversation_id: String, pinned: bool) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    update(conversation_id, json!({ "pinned": pinned }))
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_move_conversation(
    conversation_id: String,
    section_id: Option<String>,
) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    if let Some(section_id) = &section_id {
        if !validate::identifier(section_id) {
            return problem("bad_section");
        }
    }
    update(
        conversation_id,
        json!({ "section_id": section_id.map(serde_json::Value::String) }),
    )
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_archive_conversation(conversation_id: String, archived: bool) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    update(conversation_id, json!({ "archived": archived }))
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_timeline(conversation_id: String, before_seq: Option<i64>) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    let mut target = format!("/v1/chat/conversations/{conversation_id}/timeline");
    if let Some(before_seq) = before_seq {
        if before_seq < 0 {
            return problem("invalid_cursor");
        }
        target.push_str(&format!("?before_seq={before_seq}"));
    }
    get(target)
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_messages(
    conversation_id: String,
    after_seq: Option<i64>,
    before_seq: Option<i64>,
) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    let mut params = Vec::new();
    for (name, value) in [("after_seq", after_seq), ("before_seq", before_seq)] {
        if let Some(value) = value {
            if value < 0 {
                return problem("invalid_cursor");
            }
            params.push(format!("{name}={value}"));
        }
    }
    let suffix = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    get(format!(
        "/v1/chat/conversations/{conversation_id}/messages{suffix}"
    ))
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_send(
    conversation_id: String,
    client_id: String,
    text: String,
    reply_to_id: Option<String>,
) -> ChatPlan {
    plan_chat_send_with_attachments(conversation_id, client_id, text, reply_to_id, Vec::new())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_read(conversation_id: String, last_seq: i64) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    if last_seq < 0 {
        return problem("invalid_cursor");
    }
    post(
        format!("/v1/chat/conversations/{conversation_id}/read"),
        json!({ "last_seq": last_seq })
            .as_object()
            .cloned()
            .unwrap_or_default(),
    )
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_sections() -> ChatPlan {
    get("/v1/chat/sections".to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_section(section_id: Option<String>, name: String) -> ChatPlan {
    if let Some(section_id) = &section_id {
        if !validate::identifier(section_id) {
            return problem("bad_section");
        }
    }
    let name = match clean_section_name(&name) {
        Ok(name) => name,
        Err(code) => return problem(code),
    };
    let mut body = serde_json::Map::new();
    if let Some(section_id) = section_id {
        body.insert("id".to_string(), json!(section_id));
    }
    body.insert("name".to_string(), json!(name));
    post("/v1/chat/sections".to_string(), body)
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_delegation(delegation_id: String) -> ChatPlan {
    if !validate::identifier(&delegation_id) {
        return problem("bad_delegation");
    }
    get(format!("/v1/chat/delegations/{delegation_id}"))
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_cancel_delegation(delegation_id: String) -> ChatPlan {
    if !validate::identifier(&delegation_id) {
        return problem("bad_delegation");
    }
    post(
        format!("/v1/chat/delegations/{delegation_id}/cancel"),
        serde_json::Map::new(),
    )
}

/// The stop route takes no body; the answer says whether a turn was open.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_stop(conversation_id: String) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    bodiless(
        "POST",
        format!("/v1/chat/conversations/{conversation_id}/stop"),
    )
}

/// Deletes for good; a conversation with an open turn, delegation or approval
/// is refused with `409 conversation_busy`.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_delete_conversation(conversation_id: String) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    bodiless(
        "DELETE",
        format!("/v1/chat/conversations/{conversation_id}"),
    )
}

/// One opaque id per send attempt. A transport failure reuses it (the host may
/// have stored the message); an HTTP refusal that stored nothing gets a new one.
#[flutter_rust_bridge::frb(sync)]
pub fn new_chat_client_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("c-{nanos:x}-{counter:x}-{:x}", std::process::id())
}

pub(crate) fn get(target: String) -> ChatPlan {
    ChatPlan {
        request: Some(HttpCall {
            method: "GET".to_string(),
            target,
            body: Vec::new(),
        }),
        problem: None,
    }
}

pub(crate) fn bodiless(method: &str, target: String) -> ChatPlan {
    ChatPlan {
        request: Some(HttpCall {
            method: method.to_string(),
            target,
            body: Vec::new(),
        }),
        problem: None,
    }
}

pub(crate) fn post(target: String, body: serde_json::Map<String, serde_json::Value>) -> ChatPlan {
    match serde_json::to_vec(&serde_json::Value::Object(body)) {
        Ok(bytes) => ChatPlan {
            request: Some(HttpCall {
                method: "POST".to_string(),
                target,
                body: bytes,
            }),
            problem: None,
        },
        Err(_) => problem("invalid_body"),
    }
}

fn update(conversation_id: String, patch: serde_json::Value) -> ChatPlan {
    let body = patch.as_object().cloned().unwrap_or_default();
    post(
        format!("/v1/chat/conversations/{conversation_id}/update"),
        body,
    )
}

pub(crate) fn problem(code: &str) -> ChatPlan {
    let info = action_problem(code).unwrap_or(ErrorInfo {
        message: "La petición no es válida.".to_string(),
        action: None,
        kind: FailureKind::Retry,
    });
    ChatPlan {
        request: None,
        problem: Some(info),
    }
}

fn clean_title(raw: &str) -> Result<String, &'static str> {
    let title = raw.trim();
    if title.is_empty() || title.chars().count() > 200 || title.chars().any(char::is_control) {
        return Err("bad_title");
    }
    Ok(title.to_string())
}

fn clean_section_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("bad_section_name");
    }
    Ok(name.to_string())
}

/// RFC 3986 unreserved bytes plus everything a UTF-8 query can carry.
fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(plan: ChatPlan) -> HttpCall {
        plan.request.expect("request")
    }

    #[test]
    fn the_list_encodes_the_search_and_the_filters() {
        let request = call(plan_chat_conversations(
            Some("plan de la api".to_string()),
            Some("sec_1".to_string()),
            Some(true),
            Some(false),
            false,
            false,
            None,
        ));
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.target,
            "/v1/chat/conversations?q=plan%20de%20la%20api&section_id=sec_1&pinned=true&archived=false&limit=100"
        );
        assert!(request.body.is_empty());
    }

    #[test]
    fn attention_and_working_are_sent_only_when_set_and_counts_is_its_own_route() {
        let attention = call(plan_chat_conversations(
            None, None, None, None, true, false, None,
        ));
        assert_eq!(
            attention.target,
            "/v1/chat/conversations?attention=1&limit=100"
        );
        let working = call(plan_chat_conversations(
            None, None, None, None, false, true, None,
        ));
        assert_eq!(working.target, "/v1/chat/conversations?working=1&limit=100");
        let plain = call(plan_chat_conversations(
            None, None, None, None, false, false, None,
        ));
        assert_eq!(plain.target, "/v1/chat/conversations?limit=100");
        assert_eq!(
            call(plan_chat_counts()).target,
            "/v1/chat/conversations/counts"
        );
    }

    #[test]
    fn create_rename_pin_move_and_archive_build_the_contract_bodies() {
        let create = call(plan_chat_create_conversation(Some(" Plan ".into())));
        assert_eq!(create.target, "/v1/chat/conversations");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&create.body).unwrap(),
            json!({"title":"Plan"})
        );

        let rename = call(plan_chat_rename_conversation("cnv_1".into(), "Otro".into()));
        assert_eq!(rename.target, "/v1/chat/conversations/cnv_1/update");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&rename.body).unwrap(),
            json!({"title":"Otro"})
        );

        let pin = call(plan_chat_pin_conversation("cnv_1".into(), true));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&pin.body).unwrap(),
            json!({"pinned":true})
        );

        let mov = call(plan_chat_move_conversation(
            "cnv_1".into(),
            Some("sec_2".into()),
        ));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&mov.body).unwrap(),
            json!({"section_id":"sec_2"})
        );

        let clear = call(plan_chat_move_conversation("cnv_1".into(), None));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&clear.body).unwrap(),
            json!({"section_id":null})
        );

        let archive = call(plan_chat_archive_conversation("cnv_1".into(), true));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&archive.body).unwrap(),
            json!({"archived":true})
        );
    }

    #[test]
    fn stop_posts_an_empty_body_to_the_open_turn() {
        let request = call(plan_chat_stop("cnv_1".into()));
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v1/chat/conversations/cnv_1/stop");
        assert!(request.body.is_empty());
        assert!(plan_chat_stop("../cnv_1".into()).problem.is_some());
    }

    #[test]
    fn delete_is_a_bodiless_delete_of_the_conversation() {
        let request = call(plan_chat_delete_conversation("cnv_1".into()));
        assert_eq!(request.method, "DELETE");
        assert_eq!(request.target, "/v1/chat/conversations/cnv_1");
        assert!(request.body.is_empty());
        assert!(plan_chat_delete_conversation("../cnv_1".into())
            .problem
            .is_some());
    }

    #[test]
    fn send_is_idempotent_and_validated_locally() {
        let request = call(plan_chat_send(
            "cnv_1".into(),
            "c-1".into(),
            "  hola  ".into(),
            None,
        ));
        assert_eq!(request.target, "/v1/chat/conversations/cnv_1/messages");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
            json!({"client_id":"c-1","text":"hola"})
        );

        assert_eq!(
            plan_chat_send("cnv_1".into(), "c-1".into(), "   ".into(), None)
                .problem
                .unwrap()
                .message,
            "El mensaje está vacío."
        );
        assert_eq!(
            plan_chat_send("cnv_1".into(), "c-1".into(), "a\u{7}".into(), None)
                .problem
                .unwrap()
                .message,
            "El mensaje contiene caracteres de control."
        );
        assert!(
            plan_chat_send("cnv/../x".into(), "c-1".into(), "hola".into(), None)
                .problem
                .is_some()
        );
        assert!(
            plan_chat_send("cnv_1".into(), "bad\nid".into(), "hola".into(), None)
                .problem
                .is_some()
        );
    }

    #[test]
    fn a_reply_adds_reply_to_and_a_plain_send_does_not() {
        let reply = call(plan_chat_send(
            "cnv_1".into(),
            "c-2".into(),
            "Sí, créalo".into(),
            Some("msg_01K".into()),
        ));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&reply.body).unwrap(),
            json!({
                "client_id": "c-2",
                "text": "Sí, créalo",
                "reply_to": {"kind": "message", "id": "msg_01K"}
            })
        );
        assert!(plan_chat_send(
            "cnv_1".into(),
            "c-2".into(),
            "hola".into(),
            Some("bad\nid".into())
        )
        .problem
        .is_some());
    }

    #[test]
    fn timeline_and_messages_page_with_the_seq_cursors() {
        let older = call(plan_chat_timeline("cnv_1".into(), Some(12)));
        assert_eq!(
            older.target,
            "/v1/chat/conversations/cnv_1/timeline?before_seq=12"
        );
        let newer = call(plan_chat_messages("cnv_1".into(), Some(7), None));
        assert_eq!(
            newer.target,
            "/v1/chat/conversations/cnv_1/messages?after_seq=7"
        );
    }

    #[test]
    fn read_and_sections_follow_the_contract() {
        let read = call(plan_chat_read("cnv_1".into(), 9));
        assert_eq!(read.target, "/v1/chat/conversations/cnv_1/read");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&read.body).unwrap(),
            json!({"last_seq":9})
        );

        let list = call(plan_chat_sections());
        assert_eq!(list.target, "/v1/chat/sections");

        let create = call(plan_chat_section(None, "Trabajo".into()));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&create.body).unwrap(),
            json!({"name":"Trabajo"})
        );

        let rename = call(plan_chat_section(Some("sec_1".into()), "Profundo".into()));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&rename.body).unwrap(),
            json!({"id":"sec_1","name":"Profundo"})
        );
    }

    #[test]
    fn titles_and_sections_reject_blank_and_control_characters() {
        assert!(plan_chat_create_conversation(Some("  ".into()))
            .problem
            .is_some());
        assert!(plan_chat_rename_conversation("cnv_1".into(), "a\nb".into())
            .problem
            .is_some());
        assert!(plan_chat_section(None, " ".into()).problem.is_some());
        assert!(plan_chat_section(Some("sec/1".into()), "ok".into())
            .problem
            .is_some());
    }

    #[test]
    fn delegation_detail_and_cancel_follow_the_contract() {
        let detail = call(plan_chat_delegation("dlg_1".into()));
        assert_eq!(detail.method, "GET");
        assert_eq!(detail.target, "/v1/chat/delegations/dlg_1");
        assert!(detail.body.is_empty());

        let cancel = call(plan_chat_cancel_delegation("dlg_1".into()));
        assert_eq!(cancel.method, "POST");
        assert_eq!(cancel.target, "/v1/chat/delegations/dlg_1/cancel");
        assert_eq!(cancel.body, b"{}");

        let bad = plan_chat_delegation("../secret".into());
        assert!(bad.problem.unwrap().message.contains("delegación"));
    }

    #[test]
    fn client_ids_are_unique_and_url_safe() {
        let first = new_chat_client_id();
        let second = new_chat_client_id();
        assert_ne!(first, second);
        assert!(validate::client_id(&first));
        assert!(first.len() <= 128);
    }
}
