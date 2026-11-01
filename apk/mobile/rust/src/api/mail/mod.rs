//! `/v1/mail` request plans for the operator's draft: the send, its confirmation
//! and the edit. Route, body and the local checks are built here (contract
//! `mail.md`, «Envío»); the host has the last word. Dart only signs and sends.

use serde_json::{json, Value};

use crate::api::chat::plan::{post, ChatPlan};
use crate::api::error::{info, FailureKind};
use crate::api::validate;

/// The longest subject the host takes (`crates/mail`: 998 characters).
const SUBJECT_MAX_CHARS: usize = 998;
/// The longest body the host takes (`mail.md`: 32 768 characters).
const BODY_MAX_CHARS: usize = 32_768;
const BAD_DRAFT: &str = "Ese borrador no es válido.";
const BAD_APPROVAL: &str = "Esa aprobación no es válida.";

/// Asks for approval of a send. Nothing leaves with this call.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_draft_send(account: String, draft_id: String) -> ChatPlan {
    if account.trim().is_empty() || !validate::draft_id(&draft_id) {
        return problem(BAD_DRAFT);
    }
    json_post(
        &draft_path(&draft_id, "send"),
        json!({ "account": account }),
    )
}

/// Uses the approval the operator gave in the chat. This call schedules the send.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_draft_confirm(account: String, draft_id: String, approval_id: String) -> ChatPlan {
    if account.trim().is_empty() || !validate::draft_id(&draft_id) {
        return problem(BAD_DRAFT);
    }
    if !validate::identifier(&approval_id) {
        return problem(BAD_APPROVAL);
    }
    json_post(
        &draft_path(&draft_id, "send"),
        json!({ "account": account, "approval_id": approval_id }),
    )
}

/// Replaces the content of a draft. The host closes any open send on it.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_draft_update(
    account: String,
    draft_id: String,
    to: Vec<String>,
    cc: Vec<String>,
    subject: String,
    body: String,
) -> ChatPlan {
    if account.trim().is_empty() || !validate::draft_id(&draft_id) {
        return problem(BAD_DRAFT);
    }
    if let Some(message) = edit_problem(&to, &cc, &subject, &body) {
        return problem(&message);
    }
    json_post(
        &draft_path(&draft_id, "update"),
        json!({ "account": account, "to": to, "cc": cc, "subject": subject, "body": body }),
    )
}

/// The first problem with the content of a draft, in Spanish; `None` when it may go.
fn edit_problem(to: &[String], cc: &[String], subject: &str, body: &str) -> Option<String> {
    if to.is_empty() {
        return Some("Falta al menos un destinatario.".to_string());
    }
    if let Some(address) = to.iter().chain(cc).find(|address| !bare_address(address)) {
        return Some(format!("«{address}» no es una dirección de correo válida."));
    }
    if subject.chars().count() > SUBJECT_MAX_CHARS || subject.chars().any(char::is_control) {
        return Some(format!(
            "El asunto no puede tener más de {SUBJECT_MAX_CHARS} caracteres ni saltos de línea."
        ));
    }
    let controlled = body
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'));
    if body.chars().count() > BODY_MAX_CHARS || controlled {
        return Some(format!(
            "El cuerpo no puede tener más de {BODY_MAX_CHARS} caracteres ni caracteres de control."
        ));
    }
    None
}

/// A bare address: no spaces, no controls, one `@`-separated part at least.
fn bare_address(address: &str) -> bool {
    !address.trim().is_empty()
        && address.trim() == address
        && address.contains('@')
        && !address
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
}

fn draft_path(draft_id: &str, action: &str) -> String {
    format!("/v1/mail/drafts/{}/{action}", percent_encoded(draft_id))
}

/// One path segment: everything outside the unreserved set becomes `%XX`, as
/// the Dart side does with `Uri.encodeComponent` when it reads a draft.
fn percent_encoded(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn json_post(target: &str, body: Value) -> ChatPlan {
    post(
        target.to_string(),
        body.as_object().cloned().unwrap_or_default(),
    )
}

fn problem(message: &str) -> ChatPlan {
    ChatPlan {
        request: None,
        problem: Some(info(message, None, FailureKind::Retry)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    fn update(to: &[&str], subject: &str, body: &str) -> ChatPlan {
        plan_draft_update(
            "trabajo".to_string(),
            "dft_1".to_string(),
            text(to),
            Vec::new(),
            subject.to_string(),
            body.to_string(),
        )
    }

    #[test]
    fn a_valid_edit_posts_the_whole_draft_to_its_update_route() {
        let plan = update(&["bea@ejemplo.org"], "Presupuesto", "Hola\nBea");
        let call = plan.request.expect("request");
        assert_eq!(call.method, "POST");
        assert_eq!(call.target, "/v1/mail/drafts/dft_1/update");
        let body: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(body["account"], "trabajo");
        assert_eq!(body["to"][0], "bea@ejemplo.org");
        assert_eq!(body["subject"], "Presupuesto");
        assert_eq!(body["body"], "Hola\nBea");
    }

    #[test]
    fn a_graph_sized_draft_id_is_sent_percent_encoded() {
        let graph = format!("AQMkADAw{}==", "x".repeat(130));
        let send = plan_draft_send("personal".to_string(), graph.clone());
        let call = send.request.expect("request");
        assert_eq!(
            call.target,
            format!("/v1/mail/drafts/{}%3D%3D/send", &graph[..138])
        );
        assert!(
            plan_draft_send("personal".to_string(), "a/../b".to_string())
                .request
                .is_none()
        );
        assert!(
            plan_draft_send("personal".to_string(), "con espacio".to_string())
                .request
                .is_none()
        );
    }

    #[test]
    fn an_edit_without_recipients_or_with_a_bad_address_is_refused_in_spanish() {
        assert!(update(&[], "Hola", "x").problem.is_some());
        let plan = update(&["bea ejemplo.org"], "Hola", "x");
        assert!(plan.request.is_none());
        let message = plan.problem.expect("problem").message;
        assert!(message.contains("bea ejemplo.org"));
        assert!(message.contains("no es una dirección"));
    }

    #[test]
    fn subject_and_body_limits_count_characters_not_bytes() {
        let subject = "é".repeat(SUBJECT_MAX_CHARS);
        assert!(update(&["a@b.c"], &subject, "x").request.is_some());
        let too_long = "é".repeat(SUBJECT_MAX_CHARS + 1);
        assert!(update(&["a@b.c"], &too_long, "x").request.is_none());
        assert!(update(&["a@b.c"], "Hola", "línea\tuna\r\nlínea dos")
            .request
            .is_some());
        assert!(update(&["a@b.c"], "Hola", "nulo\u{0}").request.is_none());
        assert!(update(&["a@b.c"], "dos\nlíneas", "x").request.is_none());
    }

    #[test]
    fn send_and_confirm_refuse_ids_that_could_change_the_route() {
        let send = plan_draft_send("trabajo".to_string(), "dft_1/../x".to_string());
        assert!(send.request.is_none());
        let confirm = plan_draft_confirm(
            "trabajo".to_string(),
            "dft_1".to_string(),
            "apr 1".to_string(),
        );
        assert!(confirm.request.is_none());
        let ok = plan_draft_confirm(
            "trabajo".to_string(),
            "dft_1".to_string(),
            "apr_1".to_string(),
        );
        let body: Value = serde_json::from_slice(&ok.request.expect("request").body).expect("json");
        assert_eq!(body, json!({"account": "trabajo", "approval_id": "apr_1"}));
    }
}
