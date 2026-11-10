//! The contacts of the agenda over REST (AGENDA-REST) and the phone-book import: the
//! bodies a device sends, the JSON it reads, and the store calls behind them. The tool
//! parser checks every field; an import entry that is refused is reported by its code
//! and its position, never by the address it carried.

use excubia_core_chat::{ChatError, ChatStore, Contact, ImportOutcome};
use excubia_core_seat::{ContactCall, ToolCall};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agenda_api::{self, map_error};
use crate::error::ApiError;

/// The most entries one import may carry.
const IMPORT_MAX: usize = 500;

/// A contact a device saves. `name` is required, on a create and on an update alike.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactBody {
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    addresses: Option<Vec<String>>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBody {
    contacts: Vec<ImportEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportEntry {
    name: String,
    #[serde(default)]
    addresses: Vec<String>,
}

/// Why an import entry was not written: a code the report names, or a fault that ends
/// the whole import.
enum EntryFailure {
    Refused(&'static str),
    Failed(ApiError),
}

/// Creates the contact when `contact_id` is `None`, and changes that contact otherwise.
pub fn save(
    chat: &ChatStore,
    device_id: &str,
    contact_id: Option<&str>,
    payload: ContactBody,
) -> Result<Contact, ApiError> {
    let arguments = json!({
        "id": contact_id,
        "name": payload.name,
        "aliases": payload.aliases,
        "addresses": payload.addresses,
        "notes": payload.notes,
    });
    let ToolCall::Contact(ContactCall::Save { id, fields }) =
        agenda_api::parse("contact.save", arguments)?
    else {
        return Err(ApiError::internal("contact.save parsed to another call"));
    };
    chat.save_contact_by_device(device_id, id.as_deref(), fields)
        .map_err(map_error)
}

/// Imports every entry in order, each in its own transaction. The report counts the
/// outcomes and names the refused entries by position and code.
pub fn import(chat: &ChatStore, device_id: &str, payload: ImportBody) -> Result<Value, ApiError> {
    if payload.contacts.len() > IMPORT_MAX {
        return Err(ApiError::invalid_field(
            "import_limit",
            "contacts",
            format!("an import holds at most {IMPORT_MAX} contacts"),
        ));
    }
    let (mut created, mut merged, mut skipped) = (0_usize, 0_usize, 0_usize);
    let mut rejected = Vec::new();
    for (index, entry) in payload.contacts.into_iter().enumerate() {
        match import_entry(chat, device_id, entry) {
            Ok(ImportOutcome::Created) => created += 1,
            Ok(ImportOutcome::Merged) => merged += 1,
            Ok(ImportOutcome::Skipped) => skipped += 1,
            Err(EntryFailure::Refused(code)) => {
                rejected.push(json!({"index": index, "code": code}))
            }
            Err(EntryFailure::Failed(error)) => return Err(error),
        }
    }
    Ok(json!({
        "ok": true,
        "created": created,
        "merged": merged,
        "skipped": skipped,
        "rejected": rejected,
    }))
}

fn import_entry(
    chat: &ChatStore,
    device_id: &str,
    entry: ImportEntry,
) -> Result<ImportOutcome, EntryFailure> {
    if entry.addresses.is_empty() {
        return Err(EntryFailure::Refused("no_address"));
    }
    let arguments = json!({"name": entry.name, "addresses": entry.addresses});
    let fields = match agenda_api::parse("contact.save", arguments) {
        Ok(ToolCall::Contact(ContactCall::Save { fields, .. })) => fields,
        Ok(_) => {
            return Err(EntryFailure::Failed(ApiError::internal(
                "contact.save parsed to another call",
            )))
        }
        Err(error) => return Err(EntryFailure::Refused(error.code)),
    };
    chat.import_contact_by_device(device_id, fields)
        .map_err(|error| match refusal_code(&error) {
            Some(code) => EntryFailure::Refused(code),
            None => EntryFailure::Failed(map_error(error)),
        })
}

/// The refusals an entry reports by code. Any other store error is a fault.
fn refusal_code(error: &ChatError) -> Option<&'static str> {
    match error {
        ChatError::InvalidAddress => Some("invalid_address"),
        ChatError::AddressLimit => Some("address_limit"),
        ChatError::AddressTaken(_) => Some("address_taken"),
        ChatError::ContactLimit => Some("contact_limit"),
        _ => None,
    }
}

pub fn contact_json(contact: &Contact) -> Value {
    json!({
        "id": contact.id,
        "name": contact.name,
        "aliases": contact.aliases,
        "addresses": contact
            .addresses
            .iter()
            .map(|item| json!({"address": item.address, "source": item.source.as_str()}))
            .collect::<Vec<_>>(),
        "notes": contact.notes,
        "created_at": contact.created_at,
        "updated_at": contact.updated_at,
    })
}
