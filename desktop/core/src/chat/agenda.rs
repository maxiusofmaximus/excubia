//! The agenda over REST: the app's notes and contacts (`/v1/notes`,
//! `/v1/contacts`). Reads need `READ_STATE`, writes `RESPOND`, deletes `APPROVE`.
//! Every call is signed like the rest of the chat routes.

use reqwest::Method;
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::{json, Value};

use super::agenda_model::{Contact, ContactDraft, ContactPage, Note, NoteDraft, NotePage};
use super::error::{protocol, ChatFailure, ChatResult};
use super::validate_prefixed_id;
use crate::client::HostClient;

/// The codes the agenda refuses with. A body whose `detail.code` is one of them
/// is an agenda refusal; any other code keeps its chat meaning.
const AGENDA_CODES: &[&str] = &[
    "not_found",
    "note_limit",
    "contact_limit",
    "address_taken",
    "alias_taken",
    "address_limit",
    "alias_limit",
    "invalid_address",
    "invalid_arguments",
    "invalid_body",
    "empty_message",
    "unknown_message",
    "timezone_unset",
    "timezone_already_set",
    "invalid_zone",
    "invalid_local_time",
    "calendar_limit",
    "unsupported_recurrence",
    "invalid_recurrence",
    "too_many_events",
    "invalid_ics",
];

/// One collection of the agenda: its route, the prefix of its ids, the word that
/// names them in a refusal, and the query key of its cursor.
struct Collection {
    path: &'static str,
    prefix: &'static str,
    label: &'static str,
    cursor: &'static str,
}

const NOTES: Collection = Collection {
    path: "notes",
    prefix: "nt_",
    label: "nota",
    cursor: "before",
};

const CONTACTS: Collection = Collection {
    path: "contacts",
    prefix: "ct_",
    label: "contacto",
    cursor: "after",
};

#[derive(Debug, Deserialize)]
struct NoteEnvelope {
    note: Note,
}

#[derive(Debug, Deserialize)]
struct ContactEnvelope {
    contact: Contact,
}

/// An agenda refusal, if the body names one. The host's English message stays
/// in the failure; the screen words the code itself.
pub(super) fn agenda_refusal(body: &[u8]) -> Option<ChatFailure> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let detail = value.get("detail")?.as_object()?;
    let code = detail.get("code")?.as_str()?;
    if !AGENDA_CODES.contains(&code) {
        return None;
    }
    Some(ChatFailure::AgendaRefused {
        code: code.to_string(),
        field: detail
            .get("field")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: detail
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// The query of a list. The cursor is checked before it leaves the device; an
/// empty search is not sent at all.
fn list_query(
    collection: &Collection,
    search: Option<&str>,
    cursor: Option<&str>,
) -> ChatResult<Vec<(&'static str, String)>> {
    let mut query = Vec::new();
    if let Some(text) = search.filter(|text| !text.is_empty()) {
        query.push(("q", text.to_string()));
    }
    if let Some(id) = cursor {
        validate_prefixed_id(id, collection.prefix, collection.label)?;
        query.push((collection.cursor, id.to_string()));
    }
    Ok(query)
}

/// `/v1/{collection}/{id}{action}`, with the id checked first.
fn entry_path(collection: &Collection, id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, collection.prefix, collection.label)?;
    Ok(format!("/v1/{}/{id}{action}", collection.path))
}

impl HostClient {
    pub async fn notes(&self, search: Option<&str>, before: Option<&str>) -> ChatResult<NotePage> {
        let query = list_query(&NOTES, search, before)?;
        self.chat_get("/v1/notes", &query).await
    }

    pub async fn note(&self, id: &str) -> ChatResult<Note> {
        let path = entry_path(&NOTES, id, "")?;
        let envelope: NoteEnvelope = self.chat_get(&path, &[]).await?;
        Ok(envelope.note)
    }

    pub async fn note_create(&self, draft: &NoteDraft) -> ChatResult<Note> {
        let mut body = json!({ "body": draft.body });
        if let Some(title) = &draft.title {
            body["title"] = json!(title);
        }
        if let Some(message_id) = &draft.message_id {
            body["message_id"] = json!(message_id);
        }
        let envelope: NoteEnvelope = self
            .chat_send_json(Method::POST, "/v1/notes", &[], &body, true)
            .await?;
        Ok(envelope.note)
    }

    /// `title: Some("")` clears the subject; `None` keeps it.
    pub async fn note_update(&self, id: &str, title: Option<&str>, body: &str) -> ChatResult<Note> {
        let path = entry_path(&NOTES, id, "/update")?;
        let mut change = json!({ "body": body });
        if let Some(title) = title {
            change["title"] = json!(title);
        }
        let envelope: NoteEnvelope = self
            .chat_send_json(Method::POST, &path, &[], &change, true)
            .await?;
        Ok(envelope.note)
    }

    pub async fn note_delete(&self, id: &str) -> ChatResult<()> {
        let path = entry_path(&NOTES, id, "/delete")?;
        self.chat_send_json::<IgnoredAny>(Method::POST, &path, &[], &json!({}), true)
            .await?;
        Ok(())
    }

    pub async fn contacts(
        &self,
        search: Option<&str>,
        after: Option<&str>,
    ) -> ChatResult<ContactPage> {
        let query = list_query(&CONTACTS, search, after)?;
        self.chat_get("/v1/contacts", &query).await
    }

    pub async fn contact(&self, id: &str) -> ChatResult<Contact> {
        let path = entry_path(&CONTACTS, id, "")?;
        let envelope: ContactEnvelope = self.chat_get(&path, &[]).await?;
        Ok(envelope.contact)
    }

    pub async fn contact_create(&self, draft: &ContactDraft) -> ChatResult<Contact> {
        let body = serde_json::to_value(draft).map_err(protocol)?;
        let envelope: ContactEnvelope = self
            .chat_send_json(Method::POST, "/v1/contacts", &[], &body, true)
            .await?;
        Ok(envelope.contact)
    }

    pub async fn contact_update(&self, id: &str, draft: &ContactDraft) -> ChatResult<Contact> {
        let path = entry_path(&CONTACTS, id, "/update")?;
        let body = serde_json::to_value(draft).map_err(protocol)?;
        let envelope: ContactEnvelope = self
            .chat_send_json(Method::POST, &path, &[], &body, true)
            .await?;
        Ok(envelope.contact)
    }

    pub async fn contact_delete(&self, id: &str) -> ChatResult<()> {
        let path = entry_path(&CONTACTS, id, "/delete")?;
        self.chat_send_json::<IgnoredAny>(Method::POST, &path, &[], &json!({}), true)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::super::error::status_failure;
    use super::*;

    fn refusal(status: StatusCode, body: &str) -> ChatFailure {
        status_failure(status, body.as_bytes(), None, true)
    }

    #[test]
    fn a_taken_address_names_the_contact_that_owns_it() {
        let body = r#"{"ok":false,"detail":{"code":"address_taken","message":"the address already belongs to the contact Ana Ruiz"}}"#;
        match refusal(StatusCode::CONFLICT, body) {
            ChatFailure::AgendaRefused { code, message, .. } => {
                assert_eq!(code, "address_taken");
                assert!(message.ends_with("Ana Ruiz"));
            }
            other => panic!("se esperaba AgendaRefused, llegó {other:?}"),
        }
    }

    #[test]
    fn an_invalid_address_keeps_the_field_it_came_from() {
        let body =
            r#"{"ok":false,"detail":{"code":"invalid_address","field":"addresses","message":"x"}}"#;
        match refusal(StatusCode::UNPROCESSABLE_ENTITY, body) {
            ChatFailure::AgendaRefused { code, field, .. } => {
                assert_eq!(code, "invalid_address");
                assert_eq!(field.as_deref(), Some("addresses"));
            }
            other => panic!("se esperaba AgendaRefused, llegó {other:?}"),
        }
    }

    #[test]
    fn a_missing_note_is_not_the_chat_refusal() {
        let body = r#"{"ok":false,"detail":{"code":"not_found","message":"no such note"}}"#;
        assert!(matches!(
            refusal(StatusCode::NOT_FOUND, body),
            ChatFailure::AgendaRefused { code, .. } if code == "not_found"
        ));
    }

    #[test]
    fn a_chat_conflict_keeps_its_meaning() {
        let body = r#"{"ok":false,"detail":{"code":"approval_expired"}}"#;
        assert!(matches!(
            refusal(StatusCode::CONFLICT, body),
            ChatFailure::ApprovalConflict { .. }
        ));
    }

    #[test]
    fn a_cursor_that_is_not_an_id_never_leaves_the_device() {
        assert!(list_query(&NOTES, None, Some("ct_1")).is_err());
        assert!(list_query(&NOTES, None, Some("nt_01JAB")).is_ok());
    }

    #[test]
    fn an_empty_search_is_not_sent() {
        assert!(list_query(&CONTACTS, Some(""), None).unwrap().is_empty());
    }

    #[test]
    fn a_contact_route_checks_its_id() {
        assert!(entry_path(&CONTACTS, "nt_1", "/update").is_err());
        assert_eq!(
            entry_path(&CONTACTS, "ct_1", "/update").unwrap(),
            "/v1/contacts/ct_1/update"
        );
    }
}
