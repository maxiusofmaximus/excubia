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

/// The title a conversation gets from its first message while it is still «Nuevo chat»:
/// the first line, without Markdown marks, cut at a word near 48 characters. `None`
/// when nothing readable is left (a message that is only an attachment).
pub fn title_from(text: &str) -> Option<String> {
    const MAX: usize = 48;
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let clean: String = line
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '>' | '[' | ']'))
        .collect();
    let words: Vec<&str> = clean.split_whitespace().collect();
    let mut title = String::new();
    for word in words {
        let next = if title.is_empty() {
            word.chars().count()
        } else {
            title.chars().count() + 1 + word.chars().count()
        };
        if next > MAX {
            if title.is_empty() {
                title = word.chars().take(MAX).collect();
            }
            title.push('…');
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(word);
    }
    let title = title
        .trim_end_matches(['.', ',', ';', ':', '?', '!', '¿', '¡'])
        .to_string();
    let mut chars = title.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod title_tests {
    use super::title_from;

    #[test]
    fn the_first_line_becomes_a_short_title() {
        assert_eq!(
            title_from("creemos un proyecto web").as_deref(),
            Some("Creemos un proyecto web")
        );
        assert_eq!(
            title_from("Cuáles son los trabajadores que podemos contratar para trabajar en un proyecto de software?").as_deref(),
            Some("Cuáles son los trabajadores que podemos…")
        );
        assert_eq!(
            title_from("\n\n**Hola**, Alfred.\nsegunda línea").as_deref(),
            Some("Hola, Alfred")
        );
        assert_eq!(title_from("   \n "), None);
        assert_eq!(
            title_from("¿Qué día es hoy?").as_deref(),
            Some("¿Qué día es hoy")
        );
    }
}
