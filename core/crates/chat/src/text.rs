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

pub const MAX_NAME_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("name is required")]
    Empty,
    #[error("name cannot exceed {MAX_NAME_CHARS} characters")]
    TooLong,
    #[error("name contains control characters")]
    ControlCharacters,
}

/// Trim and bound a display name: 1 to 40 characters and no control character,
/// tab or newline included (a name is one line).
pub fn agent_name(raw: &str) -> Result<String, NameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::ControlCharacters);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(NameError::TooLong);
    }
    Ok(name.to_string())
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

    #[test]
    fn names_are_trimmed_one_line_and_bounded() {
        assert_eq!(agent_name("  Ines  ").expect("name"), "Ines");
        assert_eq!(agent_name(" \n "), Err(NameError::Empty));
        assert_eq!(agent_name("Ines\tRíos"), Err(NameError::ControlCharacters));
        assert_eq!(agent_name("Ines\u{7}"), Err(NameError::ControlCharacters));
        assert_eq!(
            agent_name(&"ñ".repeat(MAX_NAME_CHARS + 1)),
            Err(NameError::TooLong)
        );
        assert_eq!(
            agent_name(&"ñ".repeat(MAX_NAME_CHARS))
                .expect("at the limit")
                .chars()
                .count(),
            MAX_NAME_CHARS
        );
    }
}
