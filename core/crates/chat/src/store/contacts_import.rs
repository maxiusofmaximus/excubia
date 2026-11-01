//! The phone-book import (AGENDA-REST). An entry whose name is a contact's adds its
//! addresses to that contact; any other entry creates one. Nothing is overwritten: a
//! name, a note or an alias stays as it is. An address that another contact keeps skips
//! the entry, and a refused entry writes nothing.

use rusqlite::{Connection, OptionalExtension, Transaction};

use super::agenda::{write_audit, Writer};
use super::contacts::{add_addresses, addresses_of, contact_rows, create_in, normalized, touch};
use crate::error::ChatError;
use crate::model_contact::{
    normalize_text, AddressSource, ContactFields, ImportOutcome, MAX_ADDRESSES_PER_CONTACT,
};

impl super::ChatStore {
    /// Import one entry of the phone book, in one transaction.
    pub fn import_contact_by_device(
        &self,
        device_id: &str,
        fields: ContactFields,
    ) -> Result<ImportOutcome, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let outcome = import_in(&transaction, Writer::device(device_id), fields, &now)?;
        transaction.commit()?;
        Ok(outcome)
    }
}

fn import_in(
    transaction: &Transaction<'_>,
    writer: Writer<'_>,
    fields: ContactFields,
    now: &str,
) -> Result<ImportOutcome, ChatError> {
    let raw = fields.addresses.unwrap_or_default();
    if raw.is_empty() {
        return Err(ChatError::InvalidAddress);
    }
    let target = named(transaction, &fields.name)?;
    let mut fresh = Vec::new();
    for address in normalized(&raw)? {
        match keeper_of(transaction, &address)? {
            Some(keeper) if Some(&keeper) != target.as_ref() => return Ok(ImportOutcome::Skipped),
            Some(_) => {}
            None => fresh.push(address),
        }
    }
    let Some(contact_id) = target else {
        let fields = ContactFields {
            name: fields.name,
            aliases: None,
            addresses: Some(fresh),
            notes: None,
        };
        let contact = create_in(transaction, fields, now, writer, AddressSource::Phone)?;
        write_audit(
            transaction,
            "contact",
            &contact.id,
            "create",
            writer.kind,
            writer.id,
            now,
        )?;
        return Ok(ImportOutcome::Created);
    };
    if addresses_of(transaction, &contact_id)?.len() + fresh.len() > MAX_ADDRESSES_PER_CONTACT {
        return Err(ChatError::AddressLimit);
    }
    if !fresh.is_empty() {
        add_addresses(transaction, &contact_id, &fresh, AddressSource::Phone)?;
        touch(transaction, &contact_id, writer, now)?;
        write_audit(
            transaction,
            "contact",
            &contact_id,
            "update",
            writer.kind,
            writer.id,
            now,
        )?;
    }
    Ok(ImportOutcome::Merged)
}

/// The newest contact whose normalized name is `name`, normalized the way a search reads it.
// shortcut: reads every name per entry (at most MAX_CONTACTS), an index on the normalized name when imports get slow
fn named(connection: &Connection, name: &str) -> Result<Option<String>, ChatError> {
    let needle = normalize_text(name);
    Ok(contact_rows(connection)?
        .into_iter()
        .find(|(_, stored, _)| normalize_text(stored) == needle)
        .map(|(contact_id, _, _)| contact_id))
}

/// The contact that keeps `address`, when one does.
fn keeper_of(connection: &Connection, address: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT contact_id FROM contact_address WHERE address = ?1",
            [address],
            |row| row.get(0),
        )
        .optional()?)
}
