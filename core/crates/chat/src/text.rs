//! Message text validation shared by the HTTP routes and the MCP tools. The
//! two transports shape errors differently, so this returns a typed reason and
//! lets each boundary map it (`orchestrator-chat.md` §3, §3.3).

use thiserror::Error;

pub const MAX_MESSAGE_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TextError {
    #[error("text is required")]
    Empty,
    #[error("text contains control characters")]
    ControlCharacters,
    #[error("text cannot exceed {MAX_MESSAGE_BYTES} bytes")]
    TooLong,
}

/// Trim, reject empty/control text and enforce the 32 KiB UTF-8 bound. The
/// returned string is what gets stored, so every writer applies the same rule.
pub fn message_text(raw: &str) -> Result<String, TextError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(TextError::Empty);
    }
    if text
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(TextError::ControlCharacters);
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(TextError::TooLong);
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(message_text("  hola  ").expect("text"), "hola");
        assert_eq!(message_text("   "), Err(TextError::Empty));
        assert_eq!(message_text("a\u{7}"), Err(TextError::ControlCharacters));
        assert_eq!(
            message_text(&"a".repeat(MAX_MESSAGE_BYTES + 1)),
            Err(TextError::TooLong)
        );
        assert_eq!(
            message_text(&"a".repeat(MAX_MESSAGE_BYTES))
                .expect("at the limit")
                .len(),
            MAX_MESSAGE_BYTES
        );
    }
}
