//! The `note.*` tools (F2): Alfred keeps what the user asks him to remember. The
//! parser bounds every field and refuses what it cannot bound; the host decides
//! who may call a tool and what a delete needs.

use excubia_core_chat::{
    message_text, ToolError, ToolRequest, NOTE_SEARCH_MAX_RESULTS, NOTE_SUBJECT_MAX_CHARS,
};

use super::args::{IdArgs, NoteSaveArgs, NoteSearchArgs, NoteUpdateArgs};
use super::parse::{arguments, name_field, one_line, text_error};

/// The results a search returns when the caller names no limit.
const DEFAULT_SEARCH_RESULTS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteCall {
    Save {
        subject: Option<String>,
        body: String,
    },
    Search {
        query: String,
        limit: usize,
    },
    /// A field that is `None` keeps its value. An empty subject clears it.
    Update {
        id: String,
        subject: Option<String>,
        body: Option<String>,
    },
    Delete {
        id: String,
    },
}

pub(super) fn save(request: &ToolRequest) -> Result<NoteCall, ToolError> {
    let args: NoteSaveArgs = arguments(request, "body")?;
    let body = message_text(&args.body).map_err(|error| text_error("body", error))?;
    Ok(NoteCall::Save {
        subject: subject(args.subject)?,
        body,
    })
}

pub(super) fn search(request: &ToolRequest) -> Result<NoteCall, ToolError> {
    let args: NoteSearchArgs = arguments(request, "query")?;
    let query = name_field("query", &args.query)?;
    let limit = args.limit.unwrap_or(DEFAULT_SEARCH_RESULTS);
    if !(1..=NOTE_SEARCH_MAX_RESULTS).contains(&limit) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "limit",
            format!("limit must be 1 to {NOTE_SEARCH_MAX_RESULTS}"),
        ));
    }
    Ok(NoteCall::Search { query, limit })
}

pub(super) fn update(request: &ToolRequest) -> Result<NoteCall, ToolError> {
    let args: NoteUpdateArgs = arguments(request, "id")?;
    let id = name_field("id", &args.id)?;
    let changes_subject = args.subject.is_some();
    // An empty subject clears the stored one, so an update keeps it as `Some("")`.
    let subject = args
        .subject
        .map(|raw| one_line("subject", &raw, NOTE_SUBJECT_MAX_CHARS))
        .transpose()?;
    let body = args
        .body
        .map(|raw| message_text(&raw).map_err(|error| text_error("body", error)))
        .transpose()?;
    if body.is_none() && !changes_subject {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "body",
            "update needs a body or a subject",
        ));
    }
    Ok(NoteCall::Update { id, subject, body })
}

pub(super) fn delete(request: &ToolRequest) -> Result<NoteCall, ToolError> {
    let args: IdArgs = arguments(request, "id")?;
    Ok(NoteCall::Delete {
        id: name_field("id", &args.id)?,
    })
}

/// One plain line of at most `NOTE_SUBJECT_MAX_CHARS`; an empty line means no subject.
fn subject(raw: Option<String>) -> Result<Option<String>, ToolError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = one_line("subject", &raw, NOTE_SUBJECT_MAX_CHARS)?;
    Ok((!value.is_empty()).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    #[test]
    fn a_note_keeps_its_body_and_an_optional_subject() {
        let saved = save(&request(
            "note.save",
            json!({ "body": "  presupuesto 1.250 € sin impuestos  ", "subject": "  " }),
        ))
        .expect("note");
        assert_eq!(
            saved,
            NoteCall::Save {
                subject: None,
                body: "presupuesto 1.250 € sin impuestos".to_string(),
            }
        );
        let titled = save(&request(
            "note.save",
            json!({ "body": "x", "subject": "Cliente" }),
        ))
        .expect("note");
        assert!(matches!(titled, NoteCall::Save { subject: Some(ref s), .. } if s == "Cliente"));
    }

    #[test]
    fn a_subject_or_a_body_past_its_bound_is_refused_not_cut() {
        let long = "s".repeat(NOTE_SUBJECT_MAX_CHARS + 1);
        let refused = save(&request(
            "note.save",
            json!({ "body": "x", "subject": long }),
        ))
        .expect_err("subject");
        assert_eq!(refused.field, Some("subject"));

        let empty = save(&request("note.save", json!({ "body": "   " }))).expect_err("body");
        assert_eq!(empty.code, "empty_message");

        let too_big = "b".repeat(32 * 1024 + 1);
        let big = save(&request("note.save", json!({ "body": too_big }))).expect_err("body");
        assert_eq!(big.code, "invalid_body");
    }

    #[test]
    fn a_search_bounds_its_limit_and_defaults_to_ten() {
        let default =
            search(&request("note.search", json!({ "query": "presupuesto" }))).expect("search");
        assert_eq!(
            default,
            NoteCall::Search {
                query: "presupuesto".to_string(),
                limit: 10,
            }
        );
        for limit in [0, 21] {
            let refused = search(&request(
                "note.search",
                json!({ "query": "x", "limit": limit }),
            ))
            .expect_err("limit");
            assert_eq!(refused.field, Some("limit"), "{limit}");
        }
        assert!(search(&request(
            "note.search",
            json!({ "query": "x", "limit": 20 })
        ))
        .is_ok());
    }

    #[test]
    fn an_update_changes_something_and_a_delete_names_one_note() {
        let nothing = update(&request("note.update", json!({ "id": "nt_1" }))).expect_err("empty");
        assert_eq!(nothing.code, "invalid_arguments");

        let cleared = update(&request(
            "note.update",
            json!({ "id": "nt_1", "subject": "" }),
        ))
        .expect("clear");
        // The empty subject survives as `Some("")`: the store reads it as "clear it".
        assert_eq!(
            cleared,
            NoteCall::Update {
                id: "nt_1".to_string(),
                subject: Some(String::new()),
                body: None,
            }
        );

        let deleted = delete(&request("note.delete", json!({ "id": "nt_1" }))).expect("delete");
        assert_eq!(
            deleted,
            NoteCall::Delete {
                id: "nt_1".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_field_is_refused_never_ignored() {
        let widened = delete(&request(
            "note.delete",
            json!({ "id": "nt_1", "all": true }),
        ))
        .expect_err("unknown field");
        assert_eq!(widened.code, "invalid_arguments");
    }
}
