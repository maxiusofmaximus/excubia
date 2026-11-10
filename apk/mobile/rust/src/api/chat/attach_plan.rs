//! Request plans of the attachment routes, and of a message that carries
//! attachments. As in `plan.rs`, route, body and local validation live here;
//! Dart only signs and sends the bytes.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::api::error::{ErrorInfo, FailureKind};
use crate::api::validate::{self, TextProblem};

use super::attach_upload::{CHUNK_BYTES, MAX_ATTACHMENT_BYTES};
use super::plan::{self, bodiless, get, post, ChatPlan};

const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;
const MAX_NAME_CHARS: usize = 120;

/// `POST …/attachments`: starts an upload. The size is the input's, in bytes.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_create(
    conversation_id: String,
    client_id: String,
    name: String,
    size: u64,
    sha256: String,
) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    if !validate::client_id(&client_id) {
        return problem("bad_client_id");
    }
    if !valid_name(&name) {
        return problem("invalid_name");
    }
    if size > MAX_ATTACHMENT_BYTES {
        return problem("file_too_large");
    }
    if !valid_sha256(&sha256) {
        return problem("invalid_sha256");
    }
    json_post(
        format!("/v1/chat/conversations/{conversation_id}/attachments"),
        json!({
            "client_id": client_id,
            "name": name,
            "size": size,
            "sha256": sha256.to_ascii_lowercase(),
        }),
    )
}

/// `POST …/{att}/chunks`: one chunk of raw bytes at its offset.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_chunk(
    conversation_id: String,
    attachment_id: String,
    offset: u64,
    data: Vec<u8>,
) -> ChatPlan {
    let path = match attachment_path(&conversation_id, &attachment_id) {
        Ok(path) => path,
        Err(code) => return problem(code),
    };
    if data.is_empty() || data.len() as u64 > CHUNK_BYTES {
        return problem("bad_chunk");
    }
    json_post(
        format!("{path}/chunks"),
        json!({ "offset": offset, "data": STANDARD.encode(data) }),
    )
}

/// `POST …/{att}/complete`: the host checks the bytes and stores the WebP.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_complete(conversation_id: String, attachment_id: String) -> ChatPlan {
    match attachment_path(&conversation_id, &attachment_id) {
        Ok(path) => bodiless("POST", format!("{path}/complete")),
        Err(code) => problem(code),
    }
}

/// `DELETE …/{att}`: drops an upload in progress or a file no message took.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_delete(conversation_id: String, attachment_id: String) -> ChatPlan {
    match attachment_path(&conversation_id, &attachment_id) {
        Ok(path) => bodiless("DELETE", path),
        Err(code) => problem(code),
    }
}

/// `GET …/{att}/content`: the stored WebP of a ready file.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_content(conversation_id: String, attachment_id: String) -> ChatPlan {
    match attachment_path(&conversation_id, &attachment_id) {
        Ok(path) => get(format!("{path}/content")),
        Err(code) => problem(code),
    }
}

/// `GET …/{att}/preview?page=N`: the PNG of one page, from 1.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_attachment_preview(
    conversation_id: String,
    attachment_id: String,
    page: u32,
) -> ChatPlan {
    if page == 0 {
        return problem("invalid_page");
    }
    match attachment_path(&conversation_id, &attachment_id) {
        Ok(path) => get(format!("{path}/preview?page={page}")),
        Err(code) => problem(code),
    }
}

/// The send plan of a message with up to ten ready attachments. The text may
/// be empty only when at least one attachment is named.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_chat_send_with_attachments(
    conversation_id: String,
    client_id: String,
    text: String,
    reply_to_id: Option<String>,
    attachments: Vec<String>,
) -> ChatPlan {
    if !validate::identifier(&conversation_id) {
        return problem("bad_conversation");
    }
    if !validate::client_id(&client_id) {
        return problem("bad_client_id");
    }
    if attachments.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return problem("too_many_attachments");
    }
    if !attachments.iter().all(|id| validate::identifier(id)) {
        return problem("bad_attachment");
    }
    let text = match send_text(&text, !attachments.is_empty()) {
        Ok(text) => text,
        Err(code) => return problem(code),
    };
    let mut body = json!({ "client_id": client_id, "text": text });
    if let Some(id) = reply_to_id {
        if !validate::identifier(&id) {
            return problem("bad_reply_to");
        }
        body["reply_to"] = json!({ "kind": "message", "id": id });
    }
    // Absent means none, so a message without files keeps the old body.
    if !attachments.is_empty() {
        body["attachments"] = json!(attachments);
    }
    json_post(
        format!("/v1/chat/conversations/{conversation_id}/messages"),
        body,
    )
}

/// Text rules of a send: blank text is allowed only beside an attachment, and
/// then it goes on the wire empty.
fn send_text(raw: &str, has_attachments: bool) -> Result<String, &'static str> {
    if has_attachments && raw.trim().is_empty() {
        return Ok(String::new());
    }
    validate::message_text(raw).map_err(|reason| match reason {
        TextProblem::Empty => "empty_message",
        TextProblem::ControlCharacters => "control_characters",
        TextProblem::TooLong => "message_too_long",
    })
}

/// The route of one attachment, or the code of the id that is not a path segment.
fn attachment_path(conversation_id: &str, attachment_id: &str) -> Result<String, &'static str> {
    if !validate::identifier(conversation_id) {
        return Err("bad_conversation");
    }
    if !validate::identifier(attachment_id) {
        return Err("bad_attachment");
    }
    Ok(format!(
        "/v1/chat/conversations/{conversation_id}/attachments/{attachment_id}"
    ))
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_CHARS).contains(&name.chars().count()) && !name.chars().any(char::is_control)
}

fn valid_sha256(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn json_post(target: String, body: Value) -> ChatPlan {
    post(target, body.as_object().cloned().unwrap_or_default())
}

/// Refusals this module judges itself. Shared codes keep the wording of `plan.rs`.
fn problem(code: &str) -> ChatPlan {
    let hint = |message: &str, action: &str| ErrorInfo {
        message: message.to_string(),
        action: Some(action.to_string()),
        kind: FailureKind::Retry,
    };
    let info = match code {
        "too_many_attachments" => hint(
            "Un mensaje admite hasta 10 archivos.",
            "Quita alguno y vuelve a enviarlo.",
        ),
        "file_too_large" => hint("La imagen supera los 10 MiB.", "Elige una más pequeña."),
        "invalid_name" => hint(
            "El nombre de la imagen no es válido.",
            "Usa hasta 120 caracteres sin caracteres de control.",
        ),
        "bad_attachment" => hint("Ese adjunto no es válido.", "Vuelve a subir la imagen."),
        _ => return plan::problem(code),
    };
    ChatPlan {
        request: None,
        problem: Some(info),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::write::HttpCall;

    const CONVERSATION: &str = "cnv_01J9";
    const SHA: &str = "9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08";

    fn call(plan: ChatPlan) -> HttpCall {
        plan.request.expect("request")
    }

    fn body(plan: ChatPlan) -> Value {
        serde_json::from_slice(&call(plan).body).expect("json body")
    }

    fn send(text: &str, attachments: &[&str]) -> ChatPlan {
        plan_chat_send_with_attachments(
            CONVERSATION.into(),
            "c-1".into(),
            text.into(),
            None,
            attachments.iter().map(|id| id.to_string()).collect(),
        )
    }

    #[test]
    fn create_posts_the_contract_body_with_a_lower_case_hash() {
        let plan = plan_attachment_create(
            CONVERSATION.into(),
            "5e2f".into(),
            "captura.png".into(),
            183_204,
            SHA.into(),
        );
        let request = call(plan.clone());
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.target,
            "/v1/chat/conversations/cnv_01J9/attachments"
        );
        assert_eq!(body(plan)["sha256"], SHA.to_ascii_lowercase());
    }

    #[test]
    fn create_refuses_a_bad_name_a_bad_hash_and_a_file_over_ten_mib() {
        let create = |name: &str, size: u64, sha: &str| {
            plan_attachment_create(
                CONVERSATION.into(),
                "c".into(),
                name.into(),
                size,
                sha.into(),
            )
        };
        let ok_sha = "a".repeat(64);
        assert!(create("", 1, &ok_sha).problem.is_some());
        assert!(create(&"n".repeat(121), 1, &ok_sha).problem.is_some());
        assert!(create("a\u{7}b", 1, &ok_sha).problem.is_some());
        assert!(create("a.png", 1, &"g".repeat(64)).problem.is_some());
        assert!(create("a.png", 1, "abc").problem.is_some());
        assert!(create("a.png", MAX_ATTACHMENT_BYTES, &ok_sha)
            .request
            .is_some());
        let over = create("a.png", MAX_ATTACHMENT_BYTES + 1, &ok_sha);
        assert_eq!(
            over.problem.expect("problem").message,
            "La imagen supera los 10 MiB."
        );
    }

    #[test]
    fn a_chunk_is_base64_at_its_offset_and_at_most_128_kib() {
        let plan =
            plan_attachment_chunk(CONVERSATION.into(), "att_1".into(), 131_072, vec![1, 2, 3]);
        assert_eq!(
            call(plan.clone()).target,
            "/v1/chat/conversations/cnv_01J9/attachments/att_1/chunks"
        );
        assert_eq!(body(plan), json!({"offset": 131_072, "data": "AQID"}));

        let full = vec![0u8; CHUNK_BYTES as usize];
        assert!(
            plan_attachment_chunk(CONVERSATION.into(), "att_1".into(), 0, full)
                .request
                .is_some()
        );
        let too_big = vec![0u8; CHUNK_BYTES as usize + 1];
        assert!(
            plan_attachment_chunk(CONVERSATION.into(), "att_1".into(), 0, too_big)
                .problem
                .is_some()
        );
        assert!(
            plan_attachment_chunk(CONVERSATION.into(), "att_1".into(), 0, Vec::new())
                .problem
                .is_some()
        );
    }

    #[test]
    fn a_preview_is_a_get_of_one_page_and_page_zero_is_refused() {
        let request = call(plan_attachment_preview(
            CONVERSATION.into(),
            "att_1".into(),
            3,
        ));
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.target,
            "/v1/chat/conversations/cnv_01J9/attachments/att_1/preview?page=3"
        );
        assert!(request.body.is_empty());

        let first_page = plan_attachment_preview(CONVERSATION.into(), "att_1".into(), 0);
        assert!(first_page.request.is_none());
        assert!(first_page.problem.is_some());
    }

    #[test]
    fn complete_delete_and_content_use_their_routes_and_methods() {
        let at = "/v1/chat/conversations/cnv_01J9/attachments/att_1";
        let complete = call(plan_attachment_complete(
            CONVERSATION.into(),
            "att_1".into(),
        ));
        assert_eq!(
            (complete.method.as_str(), complete.target.as_str()),
            ("POST", format!("{at}/complete").as_str())
        );
        assert!(complete.body.is_empty());

        let delete = call(plan_attachment_delete(CONVERSATION.into(), "att_1".into()));
        assert_eq!(
            (delete.method.as_str(), delete.target.as_str()),
            ("DELETE", at)
        );

        let content = call(plan_attachment_content(CONVERSATION.into(), "att_1".into()));
        assert_eq!(
            (content.method.as_str(), content.target.as_str()),
            ("GET", format!("{at}/content").as_str())
        );
    }

    #[test]
    fn an_id_that_is_not_a_path_segment_is_refused_locally() {
        assert!(plan_attachment_complete("cnv/../x".into(), "att_1".into())
            .problem
            .is_some());
        assert!(plan_attachment_delete(CONVERSATION.into(), "att 1".into())
            .problem
            .is_some());
        assert!(plan_attachment_content(CONVERSATION.into(), "".into())
            .request
            .is_none());
    }

    #[test]
    fn a_message_without_files_keeps_the_old_body() {
        assert_eq!(
            body(send("hola", &[])),
            json!({"client_id": "c-1", "text": "hola"})
        );
    }

    #[test]
    fn a_message_names_one_to_ten_attachments() {
        for count in 1..=10 {
            let ids: Vec<String> = (0..count).map(|index| format!("att_{index}")).collect();
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            let plan = send("mira", &refs);
            assert!(plan.problem.is_none());
            assert_eq!(body(plan)["attachments"], json!(ids));
        }
    }

    #[test]
    fn eleven_attachments_are_refused_locally() {
        let ids = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"];
        let plan = send("mira", &ids);
        assert!(plan.request.is_none());
        assert_eq!(
            plan.problem.expect("problem").message,
            "Un mensaje admite hasta 10 archivos."
        );
    }

    #[test]
    fn empty_text_is_allowed_only_beside_an_attachment() {
        let with_file = body(send("   ", &["att_1"]));
        assert_eq!(with_file["text"], "");
        assert_eq!(with_file["attachments"], json!(["att_1"]));

        let alone = send("  ", &[]);
        assert_eq!(
            alone.problem.expect("problem").message,
            "El mensaje está vacío."
        );
    }

    #[test]
    fn a_reply_travels_with_its_files() {
        let plan = plan_chat_send_with_attachments(
            CONVERSATION.into(),
            "c-1".into(),
            String::new(),
            Some("msg_9".into()),
            vec!["att_1".into()],
        );
        let sent = body(plan);
        assert_eq!(sent["reply_to"], json!({"kind": "message", "id": "msg_9"}));
        assert_eq!(sent["attachments"], json!(["att_1"]));
    }
}
