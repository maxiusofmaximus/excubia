//! The recipients a draft may reach (F3B). An address of the book passes. One the user
//! wrote literally in the conversation of the draft is saved as a contact of the user
//! first; any other address stops the draft before anything is written.

use rusqlite::{params, Transaction};

use super::agenda::{write_audit, Writer};
use super::contacts::{create_in, known_address};
use crate::error::ChatError;
use crate::model_contact::{
    normalize_address, AddressSource, ContactFields, CONTACT_NAME_MAX_CHARS,
};
use crate::seat::SeatActor;

/// The newest messages of a conversation that may hold the address the user wrote.
const LITERAL_WINDOW: i64 = 50;

impl super::ChatStore {
    /// The addresses a draft names that it may not reach, as the caller wrote them. When
    /// there are none, the addresses the user typed are saved as contacts in one
    /// transaction; when there is one, nothing is saved.
    pub fn admit_recipients(
        &self,
        actor: &SeatActor,
        addresses: &[String],
    ) -> Result<Vec<String>, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let mut refused = Vec::new();
        let mut typed: Vec<String> = Vec::new();
        for raw in addresses {
            let address = normalize_address(raw).ok_or(ChatError::InvalidAddress)?;
            if known_address(&transaction, &address)? || typed.contains(&address) {
                continue;
            }
            if typed_by_user(&transaction, &actor.conversation_id, &address)? {
                typed.push(address);
            } else {
                refused.push(raw.clone());
            }
        }
        if refused.is_empty() {
            for address in &typed {
                save_typed(&transaction, actor, address, &now)?;
            }
            transaction.commit()?;
        }
        Ok(refused)
    }
}

/// Whether the user wrote `address` as a whole word in the newest messages of the
/// conversation. Only the user's own messages count: an agent's text never does.
fn typed_by_user(
    transaction: &Transaction<'_>,
    conversation_id: &str,
    address: &str,
) -> Result<bool, ChatError> {
    let mut statement = transaction.prepare(
        "SELECT body_md FROM (SELECT author_kind, body_md FROM message
                               WHERE conversation_id = ?1 ORDER BY seq DESC LIMIT ?2)
          WHERE author_kind = 'user'",
    )?;
    let bodies: Vec<String> = statement
        .query_map(params![conversation_id, LITERAL_WINDOW], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(bodies.iter().any(|body| {
        body.split(is_delimiter)
            .map(|word| word.trim_matches(is_sentence_mark))
            .any(|word| normalize_address(word).as_deref() == Some(address))
    }))
}

/// The characters a literal address sits between: spaces, `,`, `<>`, `;`, parentheses
/// and quotes.
fn is_delimiter(character: char) -> bool {
    character.is_whitespace() || ",;<>()\"'«»“”".contains(character)
}

/// The punctuation a sentence may put next to an address: its end, a colon, or the
/// Spanish opening marks. A dot inside the domain is not one of them.
fn is_sentence_mark(character: char) -> bool {
    ".?!:¿¡".contains(character)
}

/// Saves an address the user typed. Its name is the local part, cut to the name limit,
/// and it has no alias. The audit line names the actor, never the address.
fn save_typed(
    transaction: &Transaction<'_>,
    actor: &SeatActor,
    address: &str,
    now: &str,
) -> Result<(), ChatError> {
    let local = address.split_once('@').map_or(address, |(local, _)| local);
    let fields = ContactFields {
        name: local.chars().take(CONTACT_NAME_MAX_CHARS).collect(),
        aliases: None,
        addresses: Some(vec![address.to_string()]),
        notes: None,
    };
    let writer = Writer::seat(actor);
    let contact = create_in(transaction, fields, now, writer, AddressSource::UserMessage)?;
    write_audit(
        transaction,
        "contact",
        &contact.id,
        "create",
        writer.kind,
        writer.id,
        now,
    )
}

#[cfg(test)]
#[path = "contacts_admit_tests.rs"]
mod tests;
