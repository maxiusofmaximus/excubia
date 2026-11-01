//! Typed answers and refusals of the attachment routes, and the attachments a
//! timeline row carries. A body that does not follow the contract parses to
//! `None`, as the other chat parsers do.

use serde_json::Value;

use crate::api::error::validation_field;

use super::model::text;
use super::reducer::object;

/// One file of a message: the timeline shape from `api.md` (Attachments).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAttachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub w: u32,
    pub h: u32,
}

/// A refused attachment call. `code` is the contract code, `offline` for a
/// transport failure, or `http_<status>` when the body names none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFailure {
    pub code: String,
    pub status: u16,
    /// The field the host refused, when it named one.
    pub field: Option<String>,
    /// Set on `offset_mismatch`: the byte count the host already holds.
    pub next_offset: Option<u64>,
    pub retry_after: Option<u32>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_attachment_error(
    status: u16,
    body: String,
    retry_after: Option<u32>,
) -> AttachmentFailure {
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let detail = value.get("detail");
    let named = detail.and_then(|detail| {
        detail
            .as_str()
            .map(str::to_string)
            .or_else(|| text(detail, "code"))
    });
    AttachmentFailure {
        code: named.unwrap_or_else(|| fallback_code(status)),
        status,
        field: validation_field(&body),
        next_offset: detail
            .and_then(|detail| detail.get("next_offset"))
            .and_then(Value::as_u64),
        retry_after,
    }
}

fn fallback_code(status: u16) -> String {
    if status == 0 {
        "offline".to_string()
    } else {
        format!("http_{status}")
    }
}

/// The id of the `201` answer; the chunks go to it.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_attachment_created(json: String) -> Option<String> {
    text(object(&json)?.get("attachment")?, "id")
}

/// The `next_offset` of a chunk answer: where the next chunk starts.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_attachment_next_offset(json: String) -> Option<u64> {
    object(&json)?.get("next_offset")?.as_u64()
}

/// The ready file of a `complete` answer. An answer that is not ready is `None`.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_attachment_completed(json: String) -> Option<ChatAttachment> {
    let value = object(&json)?;
    let attachment = value.get("attachment")?;
    if text(attachment, "state")? == "ready" {
        attachment_from(attachment)
    } else {
        None
    }
}

/// `true` when the host dropped the upload or the unsent file.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_attachment_deleted(json: String) -> bool {
    object(&json).is_some()
}

/// The `attachments` of a timeline row. `None` when the row has no such key,
/// as on every row from an older host; a malformed file is dropped alone.
pub(crate) fn attachments_from(value: &Value) -> Option<Vec<ChatAttachment>> {
    value
        .get("attachments")?
        .as_array()
        .map(|rows| rows.iter().filter_map(attachment_from).collect())
}

fn attachment_from(value: &Value) -> Option<ChatAttachment> {
    Some(ChatAttachment {
        id: text(value, "id")?,
        name: text(value, "name")?,
        mime: text(value, "mime")?,
        size: value.get("size")?.as_u64()?,
        w: dimension(value, "w")?,
        h: dimension(value, "h")?,
    })
}

fn dimension(value: &Value, field: &str) -> Option<u32> {
    u32::try_from(value.get(field)?.as_u64()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_offset_mismatch_names_the_offset_to_resume_from() {
        let body = r#"{"detail":{"code":"offset_mismatch","message":"x","next_offset":131072}}"#;
        let failure = explain_attachment_error(409, body.to_string(), None);
        assert_eq!(failure.code, "offset_mismatch");
        assert_eq!(failure.status, 409);
        assert_eq!(failure.next_offset, Some(131_072));
        assert_eq!(failure.field, None);
    }

    #[test]
    fn a_refused_field_and_a_string_detail_are_decoded() {
        let refused = r#"{"detail":{"code":"hash_mismatch","field":"sha256","message":"x"}}"#;
        let failure = explain_attachment_error(422, refused.to_string(), None);
        assert_eq!(failure.code, "hash_mismatch");
        assert_eq!(failure.field.as_deref(), Some("sha256"));

        let plain =
            explain_attachment_error(404, r#"{"detail":"no_such_attachment"}"#.into(), None);
        assert_eq!(plain.code, "no_such_attachment");
        assert_eq!(plain.next_offset, None);
    }

    #[test]
    fn a_failure_without_a_code_gets_a_status_code_or_offline() {
        let server = explain_attachment_error(500, "<html>".to_string(), Some(7));
        assert_eq!(server.code, "http_500");
        assert_eq!(server.retry_after, Some(7));
        assert_eq!(
            explain_attachment_error(0, String::new(), None).code,
            "offline"
        );
    }

    #[test]
    fn the_create_answer_gives_the_id_and_a_bad_answer_gives_none() {
        let created = r#"{"ok":true,"attachment":{"id":"att_01K","state":"uploading"}}"#;
        assert_eq!(
            parse_attachment_created(created.into()),
            Some("att_01K".into())
        );
        assert_eq!(parse_attachment_created(r#"{"ok":false}"#.into()), None);
    }

    #[test]
    fn the_chunk_answer_gives_the_next_offset() {
        assert_eq!(
            parse_attachment_next_offset(r#"{"ok":true,"next_offset":262144}"#.into()),
            Some(262_144)
        );
        assert_eq!(parse_attachment_next_offset(r#"{"ok":true}"#.into()), None);
    }

    #[test]
    fn only_a_ready_file_completes() {
        let ready = json!({
            "ok": true,
            "attachment": {"id": "att_1", "name": "captura.png", "mime": "image/webp",
                "size": 48212, "w": 1024, "h": 768, "state": "ready"}
        });
        let file = parse_attachment_completed(ready.to_string()).expect("ready");
        assert_eq!(file.name, "captura.png");
        assert_eq!((file.w, file.h, file.size), (1024, 768, 48212));

        let uploading = json!({"ok": true, "attachment": {"id": "att_1", "state": "uploading"}});
        assert_eq!(parse_attachment_completed(uploading.to_string()), None);
    }

    #[test]
    fn a_delete_is_ok_only_when_the_host_says_so() {
        assert!(parse_attachment_deleted(r#"{"ok":true}"#.into()));
        assert!(!parse_attachment_deleted(r#"{"ok":false}"#.into()));
    }

    #[test]
    fn a_timeline_row_without_attachments_still_decodes() {
        let old = json!({"seq": 3, "kind": "message"});
        assert_eq!(attachments_from(&old), None);
    }

    #[test]
    fn a_timeline_row_carries_its_files_and_drops_a_malformed_one() {
        let row = json!({"seq": 4, "attachments": [
            {"id": "att_1", "name": "a.png", "mime": "image/webp", "size": 10, "w": 2, "h": 3},
            {"id": "att_2", "name": "broken.png"}
        ]});
        let files = attachments_from(&row).expect("attachments");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, "att_1");
        assert_eq!((files[0].w, files[0].h), (2, 3));
    }
}
