//! Alfred's contacts (F3) and the ones the app keeps (AGENDA-REST): saving and deleting
//! them, and the addresses and aliases they keep. A write and its `agenda_audit` line
//! share one transaction, and the audit keeps metadata only. Alfred's delete is never
//! made here on its own: it is the approval's burn, in `agenda::delete_object`, once the
//! user grants it. The app's delete is the user's own act and runs at once.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::agenda::{write_audit, Writer};
use crate::error::ChatError;
use crate::id;
use crate::model_contact::{
    normalize_address, normalize_text, AddressSource, Contact, ContactAddress, ContactFields,
    MAX_ADDRESSES_PER_CONTACT, MAX_ALIASES_PER_CONTACT, MAX_CONTACTS,
};
use crate::seat::SeatActor;

impl super::ChatStore {
    /// Create a contact, or change the one named by `contact_id`. Each list that is
    /// given replaces the stored one; each one left out keeps it.
    pub fn save_contact(
        &self,
        actor: &SeatActor,
        contact_id: Option<&str>,
        fields: ContactFields,
    ) -> Result<Contact, ChatError> {
        self.save_as(
            Writer::seat(actor),
            AddressSource::Alfred,
            contact_id,
            fields,
        )
    }

    /// Save a contact the app wrote. Its addresses carry the app as their origin.
    pub fn save_contact_by_device(
        &self,
        device_id: &str,
        contact_id: Option<&str>,
        fields: ContactFields,
    ) -> Result<Contact, ChatError> {
        self.save_as(
            Writer::device(device_id),
            AddressSource::App,
            contact_id,
            fields,
        )
    }

    /// Delete a contact the app asks to delete. The user asked for it, so it runs at once.
    pub fn delete_contact_by_device(
        &self,
        device_id: &str,
        contact_id: &str,
    ) -> Result<(), ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        contact_by_id(&transaction, contact_id)?;
        delete_in(&transaction, contact_id, Writer::device(device_id), &now)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn contact(&self, contact_id: &str) -> Result<Contact, ChatError> {
        let connection = self.lock();
        contact_by_id(&connection, contact_id)
    }

    fn save_as(
        &self,
        writer: Writer<'_>,
        source: AddressSource,
        contact_id: Option<&str>,
        fields: ContactFields,
    ) -> Result<Contact, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let (contact, action) = match contact_id {
            Some(contact_id) => (
                update_in(&transaction, contact_id, fields, writer, source, &now)?,
                "update",
            ),
            None => (
                create_in(&transaction, fields, &now, writer, source)?,
                "create",
            ),
        };
        write_audit(
            &transaction,
            "contact",
            &contact.id,
            action,
            writer.kind,
            writer.id,
            &now,
        )?;
        transaction.commit()?;
        Ok(contact)
    }
}

/// Delete a contact inside the transaction that burns the user's approval, or that the
/// app's delete opens. Its addresses and aliases go with it. A contact already gone is no
/// failure here.
pub(super) fn delete_in(
    transaction: &Transaction<'_>,
    contact_id: &str,
    writer: Writer<'_>,
    at: &str,
) -> Result<(), ChatError> {
    if transaction.execute("DELETE FROM contact WHERE id = ?1", [contact_id])? == 1 {
        write_audit(
            transaction,
            "contact",
            contact_id,
            "delete",
            writer.kind,
            writer.id,
            at,
        )?;
    }
    Ok(())
}

/// Inserts a contact with its addresses, which carry `source`. The caller writes the audit.
pub(super) fn create_in(
    transaction: &Transaction<'_>,
    fields: ContactFields,
    now: &str,
    writer: Writer<'_>,
    source: AddressSource,
) -> Result<Contact, ChatError> {
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM contact", [], |row| row.get(0))?;
    if count >= MAX_CONTACTS {
        return Err(ChatError::ContactLimit);
    }
    let contact_id = id::new_id("ct");
    transaction.execute(
        "INSERT INTO contact (id, name, notes, created_at, updated_at, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
        params![contact_id, fields.name, fields.notes, now, writer.kind],
    )?;
    replace_addresses(
        transaction,
        &contact_id,
        fields.addresses.unwrap_or_default(),
        source,
    )?;
    replace_aliases(transaction, &contact_id, fields.aliases.unwrap_or_default())?;
    contact_by_id(transaction, &contact_id)
}

fn update_in(
    transaction: &Transaction<'_>,
    contact_id: &str,
    fields: ContactFields,
    writer: Writer<'_>,
    source: AddressSource,
    now: &str,
) -> Result<Contact, ChatError> {
    let current = contact_by_id(transaction, contact_id)?;
    transaction.execute(
        "UPDATE contact SET name = ?2, notes = ?3, updated_at = ?4, updated_by = ?5
          WHERE id = ?1",
        params![
            contact_id,
            fields.name,
            fields.notes.or(current.notes),
            now,
            writer.kind
        ],
    )?;
    if let Some(addresses) = fields.addresses {
        replace_addresses(transaction, contact_id, addresses, source)?;
    }
    if let Some(aliases) = fields.aliases {
        replace_aliases(transaction, contact_id, aliases)?;
    }
    contact_by_id(transaction, contact_id)
}

/// Marks a contact as changed by `writer` at `now`, for a change that touches none of its
/// own fields, such as a new address.
pub(super) fn touch(
    transaction: &Transaction<'_>,
    contact_id: &str,
    writer: Writer<'_>,
    now: &str,
) -> Result<(), ChatError> {
    transaction.execute(
        "UPDATE contact SET updated_at = ?2, updated_by = ?3 WHERE id = ?1",
        params![contact_id, now, writer.kind],
    )?;
    Ok(())
}

/// Replaces the addresses of a contact. Every check runs before the first change, so
/// a refusal leaves the stored addresses as they were.
fn replace_addresses(
    transaction: &Transaction<'_>,
    contact_id: &str,
    raw: Vec<String>,
    source: AddressSource,
) -> Result<(), ChatError> {
    if raw.len() > MAX_ADDRESSES_PER_CONTACT {
        return Err(ChatError::AddressLimit);
    }
    let addresses = normalized(&raw)?;
    for address in &addresses {
        if let Some(owner) = address_owner(transaction, address, contact_id)? {
            return Err(ChatError::AddressTaken(owner));
        }
    }
    transaction.execute(
        "DELETE FROM contact_address WHERE contact_id = ?1",
        [contact_id],
    )?;
    add_addresses(transaction, contact_id, &addresses, source)
}

/// Adds addresses to a contact, each one with `source`. The caller has checked them.
pub(super) fn add_addresses(
    transaction: &Transaction<'_>,
    contact_id: &str,
    addresses: &[String],
    source: AddressSource,
) -> Result<(), ChatError> {
    for address in addresses {
        transaction.execute(
            "INSERT INTO contact_address (contact_id, address, source) VALUES (?1, ?2, ?3)",
            params![contact_id, address, source.as_str()],
        )?;
    }
    Ok(())
}

/// The addresses in their stored form, each one once. One malformed address refuses all.
pub(super) fn normalized(raw: &[String]) -> Result<Vec<String>, ChatError> {
    let addresses = raw
        .iter()
        .map(|raw| normalize_address(raw).ok_or(ChatError::InvalidAddress))
        .collect::<Result<_, _>>()?;
    Ok(distinct(addresses))
}

/// Replaces the aliases of a contact, normalized the way a search reads them.
fn replace_aliases(
    transaction: &Transaction<'_>,
    contact_id: &str,
    raw: Vec<String>,
) -> Result<(), ChatError> {
    if raw.len() > MAX_ALIASES_PER_CONTACT {
        return Err(ChatError::AliasLimit);
    }
    let aliases = distinct(raw.iter().map(|alias| normalize_text(alias)).collect());
    for alias in &aliases {
        if alias_owner(transaction, alias, contact_id)?.is_some() {
            return Err(ChatError::AliasTaken);
        }
    }
    transaction.execute(
        "DELETE FROM contact_alias WHERE contact_id = ?1",
        [contact_id],
    )?;
    for alias in aliases {
        transaction.execute(
            "INSERT INTO contact_alias (contact_id, alias) VALUES (?1, ?2)",
            params![contact_id, alias],
        )?;
    }
    Ok(())
}

/// The name of the contact that keeps `address`, when it is another one.
fn address_owner(
    connection: &Connection,
    address: &str,
    contact_id: &str,
) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT c.name FROM contact_address a JOIN contact c ON c.id = a.contact_id
              WHERE a.address = ?1 AND a.contact_id != ?2",
            params![address, contact_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// The contact that owns `alias`, when it is another one.
fn alias_owner(
    connection: &Connection,
    alias: &str,
    contact_id: &str,
) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT contact_id FROM contact_alias WHERE alias = ?1 AND contact_id != ?2",
            params![alias, contact_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Whether the book keeps `raw`, once normalized; a malformed address is not kept.
pub(super) fn known_address(connection: &Connection, raw: &str) -> Result<bool, ChatError> {
    let Some(address) = normalize_address(raw) else {
        return Ok(false);
    };
    Ok(connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM contact_address WHERE address = ?1)",
        [address],
        |row| row.get(0),
    )?)
}

/// Every contact's id, name and last change, newest first.
pub(super) fn contact_rows(
    connection: &Connection,
) -> Result<Vec<(String, String, String)>, ChatError> {
    let mut statement = connection
        .prepare("SELECT id, name, updated_at FROM contact ORDER BY updated_at DESC, id DESC")?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub(super) fn contact_by_id(
    connection: &Connection,
    contact_id: &str,
) -> Result<Contact, ChatError> {
    let contact = connection
        .query_row(
            "SELECT id, name, notes, created_at, updated_at FROM contact WHERE id = ?1",
            [contact_id],
            |row| {
                Ok(Contact {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    notes: row.get(2)?,
                    aliases: Vec::new(),
                    addresses: Vec::new(),
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| ChatError::ContactNotFound(contact_id.to_string()))?;
    Ok(Contact {
        aliases: aliases_of(connection, contact_id)?,
        addresses: addresses_of(connection, contact_id)?,
        ..contact
    })
}

pub(super) fn aliases_of(
    connection: &Connection,
    contact_id: &str,
) -> Result<Vec<String>, ChatError> {
    column_of(
        connection,
        "SELECT alias FROM contact_alias WHERE contact_id = ?1 ORDER BY rowid",
        contact_id,
    )
}

/// The addresses of a contact in the order they were added, each with its origin.
pub(super) fn addresses_of(
    connection: &Connection,
    contact_id: &str,
) -> Result<Vec<ContactAddress>, ChatError> {
    let mut statement = connection.prepare(
        "SELECT address, source FROM contact_address WHERE contact_id = ?1 ORDER BY rowid",
    )?;
    let rows: Vec<(String, String)> = statement
        .query_map([contact_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    rows.into_iter()
        .map(|(address, source)| {
            Ok(ContactAddress {
                address,
                source: AddressSource::parse(&source)
                    .ok_or(ChatError::Inconsistent("unknown address source"))?,
            })
        })
        .collect()
}

fn column_of(
    connection: &Connection,
    sql: &str,
    contact_id: &str,
) -> Result<Vec<String>, ChatError> {
    let mut statement = connection.prepare(sql)?;
    let values = statement
        .query_map([contact_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(values)
}

/// The values in the order they came, each one once.
fn distinct(values: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for value in values {
        if !kept.contains(&value) {
            kept.push(value);
        }
    }
    kept
}

#[cfg(test)]
#[path = "contacts_tests.rs"]
mod tests;
