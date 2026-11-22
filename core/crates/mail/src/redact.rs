//! The redactor every error, log and reply of the mail path goes through.
//!
//! Two rules, both deliberately blunt: a value that follows a sensitive key
//! (`access_token`, `refresh_token`, `id_token`, `device_code`, `uploadUrl`,
//! `authorization`) is replaced, and every base64url-looking run longer than
//! 40 characters is replaced even when no key names it. Tokens are long and
//! URL-safe by construction, so the second rule catches the copies that lost
//! their key on the way.

use std::fmt;

const SENSITIVE_KEYS: [&str; 8] = [
    "access_token",
    "refresh_token",
    "id_token",
    "device_code",
    "uploadurl",
    "upload_url",
    "authorization",
    "bearer",
];

const REDACTED: &str = "<redacted>";
const LONG_RUN: usize = 40;

/// Remove tokens, device codes, authorization headers and upload URLs.
pub fn redact(input: &str) -> String {
    let keyed = redact_keyed(input);
    redact_long_runs(&keyed)
}

/// Scrub every sensitive value that still has its key in front of it.
fn redact_keyed(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut spans: Vec<(usize, usize, &'static str)> = Vec::new();
    for key in SENSITIVE_KEYS {
        for (start, _) in lower.match_indices(key) {
            let end = start + key.len();
            if is_word_byte(lower.as_bytes().get(start.wrapping_sub(1)).copied())
                || is_word_byte(lower.as_bytes().get(end).copied())
            {
                continue;
            }
            spans.push((start, end, key));
        }
    }
    spans.sort_unstable_by_key(|span| span.0);

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;
    for (start, end, key) in spans {
        if start < cursor {
            continue;
        }
        let Some((value_start, value_end)) = value_span(input, end, key) else {
            continue;
        };
        out.push_str(&input[cursor..value_start]);
        out.push_str(REDACTED);
        cursor = value_end;
    }
    out.push_str(&input[cursor..]);
    out
}

fn is_word_byte(byte: Option<u8>) -> bool {
    byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// The byte span of the value that follows a sensitive key, when there is one.
fn value_span(input: &str, key_end: usize, key: &str) -> Option<(usize, usize)> {
    let rest = &input[key_end..];
    let mut chars = rest.char_indices().peekable();
    let mut space_before_value = false;
    while let Some(&(_, ch)) = chars.peek() {
        if ch.is_whitespace() {
            space_before_value = true;
            chars.next();
        } else {
            break;
        }
    }

    // A quote glued to the key closes a quoted key: `"uploadUrl": "…"`.
    if !space_before_value {
        if let Some(&(_, ch)) = chars.peek() {
            if ch == '"' || ch == '\'' {
                chars.next();
                while let Some(&(_, next)) = chars.peek() {
                    if next.is_whitespace() {
                        space_before_value = true;
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    let mut separated = space_before_value;
    if let Some(&(_, ch)) = chars.peek() {
        if ch == ':' || ch == '=' {
            separated = true;
            chars.next();
            while let Some(&(_, next)) = chars.peek() {
                if next.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }
        }
    }
    if !separated {
        return None;
    }

    let mut quote = None;
    if let Some(&(_, ch)) = chars.peek() {
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            chars.next();
        }
    }
    let relative_start = chars.peek().map_or(rest.len(), |&(index, _)| index);
    let value_start = key_end + relative_start;
    let tail = &input[value_start..];

    let value_end = if key == "authorization" {
        tail.find('\n')
            .map_or(input.len(), |offset| value_start + offset)
    } else if let Some(quote) = quote {
        tail.find(quote)
            .map_or(input.len(), |offset| value_start + offset)
    } else {
        tail.char_indices()
            .find(|(_, ch)| {
                ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | '}' | ')' | ']' | ';')
            })
            .map_or(input.len(), |(offset, _)| value_start + offset)
    };

    (value_end > value_start).then_some((value_start, value_end))
}

/// Replace base64url runs longer than 40 characters: a token keeps its shape
/// even when the error forgot the key that introduced it.
fn redact_long_runs(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut run_start: Option<usize> = None;
    for (index, ch) in input.char_indices() {
        let inside = is_base64url(ch);
        match (inside, run_start) {
            (true, None) => run_start = Some(index),
            (false, Some(start)) => {
                if index - start > LONG_RUN {
                    out.push_str(&input[cursor..start]);
                    out.push_str(REDACTED);
                    cursor = index;
                }
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run_start {
        if input.len() - start > LONG_RUN {
            out.push_str(&input[cursor..start]);
            out.push_str(REDACTED);
            cursor = input.len();
        }
    }
    out.push_str(&input[cursor..]);
    out
}

fn is_base64url(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

/// Text that is safe to show: it is scrubbed on construction and on every
/// print, so a caller that passes a token by mistake does not leak it.
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted(String);

impl Redacted {
    pub fn new(text: impl Into<String>) -> Self {
        Self(redact(&text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&redact(&self.0))
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format!("{:?}", redact(&self.0)))
    }
}

impl From<String> for Redacted {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for Redacted {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

/// A secret the rest of the crate may use but never print. The value is still
/// reachable through [`Secret::expose`], which names the intent at the call
/// site; `Display` does not exist and `Debug` shows only the marker.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret(<redacted>)")
    }
}
