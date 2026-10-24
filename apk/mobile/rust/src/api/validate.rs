//! Input rules shared by the agent write plans and the chat plans.
//!
//! The host validates the same things again; this copy exists so the phone can
//! explain a mistake without a round trip, and so both surfaces use one wording
//! for the same rule.

/// Maximum UTF-8 payload of one chat message (`docs/contracts/api.md`).
pub(crate) const MAX_MESSAGE_BYTES: usize = 32 * 1024;
const MAX_CLIENT_ID_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextProblem {
    Empty,
    ControlCharacters,
    TooLong,
}

/// Trim, then enforce non-empty, printable (newline and tab allowed) and the
/// contract's byte bound. Returns the text that goes on the wire.
pub(crate) fn message_text(raw: &str) -> Result<String, TextProblem> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(TextProblem::Empty);
    }
    if text
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(TextProblem::ControlCharacters);
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(TextProblem::TooLong);
    }
    Ok(text.to_string())
}

/// `client_id` is 1…128 characters without control characters.
pub(crate) fn client_id(raw: &str) -> bool {
    !raw.is_empty()
        && raw.chars().count() <= MAX_CLIENT_ID_CHARS
        && !raw.chars().any(char::is_control)
}

/// Host identifiers (`cnv_…`, `sec_…`) are opaque, but they are path and body
/// segments: refuse anything that could change the route or the JSON shape.
pub(crate) fn identifier(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 128
        && raw.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(message_text("  hola  ").expect("text"), "hola");
        assert_eq!(message_text("   ").unwrap_err(), TextProblem::Empty);
        assert_eq!(
            message_text("hola\u{7}").unwrap_err(),
            TextProblem::ControlCharacters
        );
        assert_eq!(
            message_text(&"a".repeat(MAX_MESSAGE_BYTES + 1)).unwrap_err(),
            TextProblem::TooLong
        );
        assert_eq!(
            message_text("linea\nsiguiente\tok").unwrap(),
            "linea\nsiguiente\tok"
        );
    }

    #[test]
    fn ids_are_opaque_but_path_safe() {
        assert!(client_id("c-1"));
        assert!(!client_id(""));
        assert!(!client_id("a\nb"));
        assert!(identifier("cnv_01J9ABC"));
        assert!(!identifier("cnv/../x"));
        assert!(!identifier("cnv 1"));
        assert!(!identifier(""));
    }
}
