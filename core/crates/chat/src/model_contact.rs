//! Alfred's contacts (F3): the address book a draft's recipients come from, and the
//! rules its fields keep. Only an address of this book can reach a draft.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// The most contacts a company keeps; the next save is refused with `contact_limit`.
pub const MAX_CONTACTS: i64 = 1_000;
pub const MAX_ADDRESSES_PER_CONTACT: usize = 10;
pub const MAX_ALIASES_PER_CONTACT: usize = 10;
/// The longest name or alias, in characters.
pub const CONTACT_NAME_MAX_CHARS: usize = 120;
/// The longest address, in characters.
pub const ADDRESS_MAX_CHARS: usize = 320;
/// The most results one `contact.find` returns.
pub const CONTACT_FIND_MAX_RESULTS: usize = 10;
/// The tool a delete question is filed under; its payload carries the `contact_id`.
pub const CONTACT_DELETE_TOOL: &str = "contact.delete";

/// Where an address of the book came from; the column keeps the same words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressSource {
    /// Alfred saved it with `contact.save`.
    Alfred,
    /// The user wrote it literally in the conversation of the draft (F3B).
    UserMessage,
    /// The phone's address book, imported by the app (AGENDA-REST).
    Phone,
    /// The app saved it by hand.
    App,
}

impl AddressSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alfred => "alfred",
            Self::UserMessage => "user_message",
            Self::Phone => "phone",
            Self::App => "app",
        }
    }

    /// The origin a column holds, or `None` for a word the schema does not admit.
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Alfred, Self::UserMessage, Self::Phone, Self::App]
            .into_iter()
            .find(|source| source.as_str() == word)
    }
}

/// An address of a contact and the origin it was kept with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactAddress {
    pub address: String,
    pub source: AddressSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    pub aliases: Vec<String>,
    pub addresses: Vec<ContactAddress>,
    pub created_at: String,
    pub updated_at: String,
}

/// What one contact of a phone import did. A refused one is an error, not an outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    /// A new contact with its new addresses.
    Created,
    /// An existing contact of the same name, to which new addresses were added.
    Merged,
    /// Every address was known already, so nothing was written.
    Skipped,
}

/// The fields of a save. A list or the notes that are `None` keep their value on an
/// update; a create keeps none of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactFields {
    pub name: String,
    pub aliases: Option<Vec<String>>,
    pub addresses: Option<Vec<String>>,
    pub notes: Option<String>,
}

/// The text the book compares: lowercase, without accents, one space between words.
pub fn normalize_text(text: &str) -> String {
    let unaccented: String = text
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect();
    unaccented
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The address as the book keeps it, or `None` when it is not `local@domain.tld`: one
/// `@`, a local part, a domain with a dot and no empty label, and no spaces or `<>,;`.
pub fn normalize_address(raw: &str) -> Option<String> {
    let address = raw.trim().to_lowercase();
    let (local, domain) = address.split_once('@')?;
    let well_formed = !local.is_empty()
        && !domain.contains('@')
        && domain.split('.').count() > 1
        && domain.split('.').all(|label| !label.is_empty())
        && address.chars().count() <= ADDRESS_MAX_CHARS
        && !address.chars().any(|character| {
            character.is_whitespace() || character.is_control() || "<>,;".contains(character)
        });
    well_formed.then_some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_folds_case_accents_and_spaces() {
        assert_eq!(
            normalize_text("  Cliente   de  MADRID "),
            "cliente de madrid"
        );
        assert_eq!(normalize_text("Ángel Núñez"), "angel nunez");
    }

    #[test]
    fn an_address_is_kept_lowercase_and_must_have_one_at_and_a_dotted_domain() {
        assert_eq!(
            normalize_address("  Ana@Example.COM ").as_deref(),
            Some("ana@example.com")
        );
        for refused in [
            "ana",
            "@example.com",
            "ana@",
            "ana@example",
            "ana@@example.com",
            "ana@.com",
            "ana@example..com",
            "ana @example.com",
            "ana@example.com,otro@example.com",
            "<ana@example.com>",
            "ana\n@example.com",
        ] {
            assert_eq!(normalize_address(refused), None, "{refused}");
        }
        let long = format!("{}@example.com", "a".repeat(ADDRESS_MAX_CHARS));
        assert_eq!(normalize_address(&long), None);
    }
}
