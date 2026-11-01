//! The send routes (MAIL2A). A human asks to send an approved draft, watches
//! the cancel window and may cancel it. Sending and cancelling take the `APPROVE`
//! grant, which no agent holds; granting or revoking the sending consent takes
//! `ADMIN`, like the login that creates the account. The host does the work
//! through [`MailActions`]; this module checks the request and shapes the answer.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use hn_core_auth::VerifiedDevice;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::mail::{DraftEdit, MailDraft, SendAnswer};
use crate::routes::mail::{check_account, failure_response, StartReply};
use crate::routes::parse_body;
use crate::ApiContext;

/// Graph message ids run to ~150 characters; the room is generous on purpose.
const MAX_ID_CHARS: usize = 512;

#[derive(Deserialize)]
struct SendBody {
    account: String,
    /// The approval the owner answered, on the second call. Absent on the first.
    #[serde(default)]
    approval_id: Option<String>,
}

#[derive(Deserialize)]
struct AccountBody {
    account: String,
}

/// The owner's edit: the account, and the fields to change. Absent fields stay.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditBody {
    account: String,
    #[serde(default)]
    to: Option<Vec<String>>,
    #[serde(default)]
    cc: Option<Vec<String>>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

/// `POST /v1/mail/drafts/{id}/send` (signed, `APPROVE`). Without `approval_id` it
/// opens the question and answers `202` with the id to answer. With it, the send
/// is scheduled (`202`, `pending`) once the owner said yes.
pub async fn send_draft(
    State(ctx): State<ApiContext>,
    Path(draft_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: SendBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = check_account(&payload.account, "account") {
        return error.into_response();
    }
    if let Err(error) = check_id(&draft_id, "draft") {
        return error.into_response();
    }
    if let Some(approval_id) = payload.approval_id.as_deref() {
        if let Err(error) = check_id(approval_id, "approval_id") {
            return error.into_response();
        }
    }
    match ctx
        .mail
        .send(&payload.account, &draft_id, payload.approval_id.as_deref())
        .await
    {
        Ok(SendAnswer::Awaiting {
            approval_id,
            expires_in,
        }) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "ok": true,
                "state": "awaiting_approval",
                "approval_id": approval_id,
                "expires_in": expires_in,
            })),
        )
            .into_response(),
        Ok(SendAnswer::Scheduled {
            send_id,
            window_seconds,
        }) => (
            StatusCode::ACCEPTED,
            Json(json!({
                "ok": true,
                "state": "pending",
                "send_id": send_id,
                "window_seconds": window_seconds,
            })),
        )
            .into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `GET /v1/mail/sends/{id}` (signed, `APPROVE`): `pending`, `sent`, `cancelled`
/// or `failed`. A `failed` send adds `failure: {code, retry}`, two fixed words from
/// `mail.md`. Nothing else about the send is served: no body, no addresses, no text
/// of the server's refusal.
pub async fn send_status(State(ctx): State<ApiContext>, Path(send_id): Path<String>) -> Response {
    if let Err(error) = check_id(&send_id, "send") {
        return error.into_response();
    }
    match ctx.mail.send_status(&send_id) {
        Ok(status) => {
            let mut body = json!({"ok": true, "send_id": send_id, "state": status.state});
            if let Some(failure) = status.failure {
                body["failure"] = json!({"code": failure.code, "retry": failure.retry});
            }
            Json(body).into_response()
        }
        Err(failure) => failure_response(failure),
    }
}

/// `GET /v1/mail/drafts/{id}?account=` (signed, `APPROVE`): the draft as the
/// owner reviews it before a send, the body in full included. It only reads:
/// the draft does not change, and no agent token holds `APPROVE`.
pub async fn draft_review(
    State(ctx): State<ApiContext>,
    Path(draft_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let account = query.get("account").map(String::as_str).unwrap_or_default();
    if let Err(error) = check_account(account, "account") {
        return error.into_response();
    }
    if let Err(error) = check_id(&draft_id, "draft") {
        return error.into_response();
    }
    match ctx.mail.draft_review(account, &draft_id).await {
        Ok(draft) => Json(json!({"ok": true, "draft": draft_json(&draft)})).into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `POST /v1/mail/drafts/{id}/update` (signed, `APPROVE`): the owner edits the draft
/// by hand. The answer is the draft as the review serves it. A draft this host did
/// not create is `404` here; the send route answers that case with its own `400`.
pub async fn draft_update(
    State(ctx): State<ApiContext>,
    Extension(verified): Extension<VerifiedDevice>,
    Path(draft_id): Path<String>,
    body: Bytes,
) -> Response {
    let payload: EditBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = check_account(&payload.account, "account") {
        return error.into_response();
    }
    if let Err(error) = check_id(&draft_id, "draft") {
        return error.into_response();
    }
    let edit = DraftEdit {
        to: payload.to,
        cc: payload.cc,
        subject: payload.subject,
        body: payload.body,
    };
    match ctx
        .mail
        .draft_update(&payload.account, &draft_id, edit, &verified.device_id)
        .await
    {
        Ok(draft) => Json(json!({"ok": true, "draft": draft_json(&draft)})).into_response(),
        Err(failure) if failure.code == "mail_unknown_draft" => {
            ApiError::coded(StatusCode::NOT_FOUND, failure.code, failure.message).into_response()
        }
        Err(failure) => failure_response(failure),
    }
}

/// The review as the API serves it. Fields are listed one by one, so a field
/// added to the host's type is not served until it is listed here.
fn draft_json(draft: &MailDraft) -> Value {
    let attachments: Vec<Value> = draft
        .attachments
        .iter()
        .map(|item| json!({"name": item.name, "size": item.size, "sha256": item.sha256}))
        .collect();
    json!({
        "account": draft.account,
        "draft_id": draft.draft_id,
        "from": draft.from,
        "to": draft.to,
        "cc": draft.cc,
        "subject": draft.subject,
        "body": draft.body,
        "content_type": draft.content_type,
        "attachments": attachments,
    })
}

/// `POST /v1/mail/sends/{id}/cancel` (signed, `APPROVE`). Cancels a send still in
/// its window; afterwards the answer is the state the send is in, unchanged.
pub async fn send_cancel(State(ctx): State<ApiContext>, Path(send_id): Path<String>) -> Response {
    if let Err(error) = check_id(&send_id, "send") {
        return error.into_response();
    }
    match ctx.mail.send_cancel(&send_id) {
        Ok(state) => Json(json!({"ok": true, "send_id": send_id, "state": state})).into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `POST /v1/mail/send/enable` (signed, `ADMIN`): starts the second device login
/// that grants sending. The answer is the same as a login start.
pub async fn send_enable_start(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: AccountBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = check_account(&payload.account, "account") {
        return error.into_response();
    }
    match ctx.mail.send_enable_start(&payload.account).await {
        Ok(login) => Json(StartReply {
            ok: true,
            login: &login,
        })
        .into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `GET /v1/mail/send/enable/status?account=` (signed, `ADMIN`): where the
/// sending consent of that account stands.
pub async fn send_enable_status(
    State(ctx): State<ApiContext>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let account = query.get("account").map(String::as_str).unwrap_or_default();
    if let Err(error) = check_account(account, "account") {
        return error.into_response();
    }
    match ctx.mail.send_enable_status(account) {
        Ok(status) => Json(json!({"ok": true, "login": status})).into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// `POST /v1/mail/send/disable` (signed, `ADMIN`): revokes the sending consent.
/// The login stays; only the send tokens are deleted.
pub async fn send_disable(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: AccountBody = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = check_account(&payload.account, "account") {
        return error.into_response();
    }
    match ctx.mail.send_disable(&payload.account) {
        Ok(()) => Json(json!({
            "ok": true,
            "account": payload.account,
            "send_enabled": false,
        }))
        .into_response(),
        Err(failure) => failure_response(failure),
    }
}

/// An id the host issued: printable, non-empty, short, no spaces.
fn check_id(value: &str, field: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.chars().count() > MAX_ID_CHARS
        || value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(ApiError::invalid_field(
            "mail_invalid_input",
            field,
            "the id is missing or malformed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::MailDraftAttachment;

    #[test]
    fn the_review_serves_exactly_the_listed_fields() {
        let draft = MailDraft {
            account: "trabajo".to_owned(),
            draft_id: "AAMk-1".to_owned(),
            from: "dueno@example.org".to_owned(),
            to: vec!["ana@example.org".to_owned()],
            cc: Vec::new(),
            subject: "Presupuesto".to_owned(),
            body: "Hola".to_owned(),
            content_type: "text".to_owned(),
            attachments: vec![MailDraftAttachment {
                name: "presupuesto.pdf".to_owned(),
                size: 12,
                sha256: Some("ab".to_owned()),
            }],
        };

        let value = draft_json(&draft);

        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "account",
                "attachments",
                "body",
                "cc",
                "content_type",
                "draft_id",
                "from",
                "subject",
                "to",
            ]
        );
        assert_eq!(
            value["attachments"],
            json!([{"name": "presupuesto.pdf", "size": 12, "sha256": "ab"}])
        );
    }

    #[test]
    fn a_graph_sized_draft_id_is_accepted() {
        let graph_id = format!("AAMkAGI2{}=", "x".repeat(131));
        assert_eq!(graph_id.chars().count(), 140);

        assert!(check_id(&graph_id, "draft").is_ok());
        assert!(check_id(&"x".repeat(MAX_ID_CHARS + 1), "draft").is_err());
        assert!(check_id("with space", "draft").is_err());
    }
}
