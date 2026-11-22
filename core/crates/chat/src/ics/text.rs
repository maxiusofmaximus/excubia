//! The text of an `.ics` line: values escaped the way RFC 5545 writes them, lines folded at 75
//! octets, and the cuts a value takes to fit the calendar's limits.

/// The most octets a physical line of a written file holds, its CRLF excluded.
const LINE_OCTETS: usize = 75;

/// Escapes a value for a content line: backslash, semicolon, comma and newline.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            ';' => out.push_str(r"\;"),
            ',' => out.push_str(r"\,"),
            '\n' => out.push_str(r"\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// Reads an escaped value back. A pair the format does not name keeps its second character.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// A value read as one line: control characters become spaces and the ends are trimmed.
pub fn one_line(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Appends a content line, folded so no physical line is over 75 octets. A fold never splits a
/// character, and each physical line ends with CRLF.
pub fn push_folded(out: &mut String, line: &str) {
    let mut width = 0;
    for ch in line.chars() {
        if width + ch.len_utf8() > LINE_OCTETS {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(ch);
        width += ch.len_utf8();
    }
    out.push_str("\r\n");
}

/// The first `max` characters of a text.
pub fn truncate_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

/// The longest prefix of a text of at most `max` bytes that ends on a character.
pub fn truncate_bytes(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
