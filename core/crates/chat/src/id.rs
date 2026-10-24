//! ULID identity: 26 Crockford-base32 characters, sortable by creation time,
//! with the entity prefix the RFC uses (`co_`, `cnv_`, `msg_`…).

use std::sync::{Mutex, OnceLock};

use ulid::Generator;

fn generator() -> &'static Mutex<Generator> {
    static GENERATOR: OnceLock<Mutex<Generator>> = OnceLock::new();
    GENERATOR.get_or_init(|| Mutex::new(Generator::new()))
}

/// Generate a bare ULID for non-entity identifiers (turn boundaries, per-run
/// tokens). The generator is monotonic within the process; an overflow of the
/// random bits falls back to incrementing, so two ids minted in the same
/// millisecond still sort in creation order.
pub fn new_ulid() -> String {
    let mut generator = generator()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    generator
        .generate()
        .unwrap_or_else(|overflow| overflow.commit_overflow_increment())
        .to_string()
        .to_ascii_uppercase()
}

/// Generate a prefixed ULID with the entity prefix the RFC uses (`co_`,
/// `cnv_`, `msg_`…).
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", new_ulid())
}

/// A deterministic id with the same shape, for tests.
#[cfg(test)]
pub fn fixed(prefix: &str, value: u128) -> String {
    format!("{prefix}_{}", ulid::Ulid(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_ulids_in_creation_order() {
        let first = new_id("cnv");
        let second = new_id("cnv");
        assert!(first.starts_with("cnv_"));
        let body = first.strip_prefix("cnv_").expect("prefix");
        assert_eq!(body.len(), 26);
        assert!(body
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
        assert!(first < second, "{first} must sort before {second}");
    }

    #[test]
    fn the_fixture_shape_matches_the_generator() {
        let id = fixed("msg", 1);
        assert!(id.starts_with("msg_"));
        assert_eq!(id.len(), 4 + 26);
    }
}
