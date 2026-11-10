//! The `contact.*` tools (F3): Alfred finds, keeps and deletes the address book that a
//! draft's recipients come from. The parser bounds every field; the host decides who
//! may call a tool, and a delete waits for the user's approval.

use excubia_core_chat::{
    message_text, ContactFields, ToolError, ToolRequest, ADDRESS_MAX_CHARS, CONTACT_NAME_MAX_CHARS,
    MAX_ADDRESSES_PER_CONTACT, MAX_ALIASES_PER_CONTACT,
};

use super::args::{ContactFindArgs, ContactSaveArgs, IdArgs};
use super::parse::{arguments, name_field, one_line, text_error};

#[derive(Clone, Debug, PartialEq)]
pub enum ContactCall {
    Find {
        query: String,
    },
    /// Without an `id` the save creates a contact; with one it changes that contact.
    Save {
        id: Option<String>,
        fields: ContactFields,
    },
    Delete {
        id: String,
    },
}

pub(super) fn find(request: &ToolRequest) -> Result<ContactCall, ToolError> {
    let args: ContactFindArgs = arguments(request, "query")?;
    Ok(ContactCall::Find {
        query: name_field("query", &args.query)?,
    })
}

pub(super) fn save(request: &ToolRequest) -> Result<ContactCall, ToolError> {
    let args: ContactSaveArgs = arguments(request, "name")?;
    let id = args.id.map(|id| name_field("id", &id)).transpose()?;
    let name = one_line("name", &args.name, CONTACT_NAME_MAX_CHARS)
        .and_then(|name| required("name", name))?;
    let aliases = list(
        "aliases",
        args.aliases,
        MAX_ALIASES_PER_CONTACT,
        CONTACT_NAME_MAX_CHARS,
    )?;
    let addresses = list(
        "addresses",
        args.addresses,
        MAX_ADDRESSES_PER_CONTACT,
        ADDRESS_MAX_CHARS,
    )?;
    let notes = args
        .notes
        .map(|raw| message_text(&raw).map_err(|error| text_error("notes", error)))
        .transpose()?;
    Ok(ContactCall::Save {
        id,
        fields: ContactFields {
            name,
            aliases,
            addresses,
            notes,
        },
    })
}

pub(super) fn delete(request: &ToolRequest) -> Result<ContactCall, ToolError> {
    let args: IdArgs = arguments(request, "id")?;
    Ok(ContactCall::Delete {
        id: name_field("id", &args.id)?,
    })
}

/// Each item is one plain line of at most `item_max` characters, and the list holds at
/// most `list_max` items. Anything past a bound is refused, never cut.
fn list(
    field: &'static str,
    raw: Option<Vec<String>>,
    list_max: usize,
    item_max: usize,
) -> Result<Option<Vec<String>>, ToolError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.len() > list_max {
        return Err(ToolError::invalid(
            "invalid_arguments",
            field,
            format!("{field} takes at most {list_max} items"),
        ));
    }
    raw.iter()
        .map(|item| one_line(field, item, item_max).and_then(|item| required(field, item)))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn required(field: &'static str, value: String) -> Result<String, ToolError> {
    if value.is_empty() {
        return Err(ToolError::invalid(
            "invalid_arguments",
            field,
            format!("{field} is required"),
        ));
    }
    Ok(value)
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
    fn a_contact_is_saved_with_its_lists_trimmed_and_optional() {
        let created = save(&request(
            "contact.save",
            json!({ "name": "  Ana Ruiz ", "aliases": [" el cliente de Madrid "], "addresses": ["ana@example.com"] }),
        ))
        .expect("save");
        assert_eq!(
            created,
            ContactCall::Save {
                id: None,
                fields: ContactFields {
                    name: "Ana Ruiz".to_string(),
                    aliases: Some(vec!["el cliente de Madrid".to_string()]),
                    addresses: Some(vec!["ana@example.com".to_string()]),
                    notes: None,
                },
            }
        );
        let changed = save(&request(
            "contact.save",
            json!({ "id": "ct_1", "name": "Ana" }),
        ))
        .expect("update");
        assert!(matches!(changed, ContactCall::Save { id: Some(ref id), .. } if id == "ct_1"));
    }

    #[test]
    fn a_name_alias_or_list_past_its_bound_is_refused_not_cut() {
        let long = "n".repeat(CONTACT_NAME_MAX_CHARS + 1);
        let refused = save(&request("contact.save", json!({ "name": long }))).expect_err("name");
        assert_eq!(refused.field, Some("name"));

        let empty = save(&request("contact.save", json!({ "name": "   " }))).expect_err("name");
        assert_eq!(empty.code, "invalid_arguments");

        let blank_alias = save(&request(
            "contact.save",
            json!({ "name": "Ana", "aliases": ["  "] }),
        ))
        .expect_err("alias");
        assert_eq!(blank_alias.field, Some("aliases"));

        let too_many = vec!["ana@example.com"; MAX_ADDRESSES_PER_CONTACT + 1];
        let crowded = save(&request(
            "contact.save",
            json!({ "name": "Ana", "addresses": too_many }),
        ))
        .expect_err("addresses");
        assert_eq!(crowded.field, Some("addresses"));
    }

    #[test]
    fn a_find_needs_a_query_and_a_delete_names_one_contact() {
        let empty = find(&request("contact.find", json!({ "query": "  " }))).expect_err("query");
        assert_eq!(empty.code, "invalid_arguments");

        let deleted = delete(&request("contact.delete", json!({ "id": "ct_1" }))).expect("delete");
        assert_eq!(
            deleted,
            ContactCall::Delete {
                id: "ct_1".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_field_is_refused_never_ignored() {
        let widened = save(&request(
            "contact.save",
            json!({ "name": "Ana", "owner": "lead" }),
        ))
        .expect_err("unknown field");
        assert_eq!(widened.code, "invalid_arguments");
    }
}
