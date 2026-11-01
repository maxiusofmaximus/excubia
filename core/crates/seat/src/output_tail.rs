//! The last bytes a CLI printed during a turn, kept for the failure record. A
//! turn that ends without `chat.reply` must say why, but the text it keeps is
//! bounded and redacted first: no run token, no hook token, no sensitive
//! `key=value` and no control characters reach the run's reason or the log.

use std::collections::VecDeque;

/// Bytes of CLI output kept per turn; older bytes are dropped first.
pub const TAIL_BYTES: usize = 4096;
/// Characters of excerpt that reach the run's reason.
pub const EXCERPT_CHARS: usize = 512;

const REDACTED: &str = "[redactado]";
/// Keys whose value is never shown, e.g. `HN_TOOL_TOKEN=...` or `"password": ...`.
const SENSITIVE_KEYS: [&str; 7] = [
    "token",
    "secret",
    "password",
    "authorization",
    "api_key",
    "apikey",
    "cookie",
];

#[derive(Default)]
pub struct OutputTail {
    bytes: VecDeque<u8>,
}

impl OutputTail {
    pub fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend(chunk);
        let excess = self.bytes.len().saturating_sub(TAIL_BYTES);
        self.bytes.drain(..excess);
    }

    /// One redacted line of the tail, at most `EXCERPT_CHARS` characters.
    pub fn excerpt(&self, secrets: &[&str]) -> String {
        let raw: Vec<u8> = self.bytes.iter().copied().collect();
        redact(&String::from_utf8_lossy(&raw), secrets)
    }
}

/// Replaces every known secret, then every sensitive `key=value` and bearer
/// token, folds control characters into spaces and cuts the result short.
pub fn redact(text: &str, secrets: &[&str]) -> String {
    let mut clean = text.to_string();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        clean = clean.replace(secret, REDACTED);
    }
    let printable: String = clean
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();

    let mut words: Vec<String> = Vec::new();
    let mut redact_next = false;
    for word in printable.split_whitespace() {
        if redact_next {
            // `Authorization: Bearer x`: the scheme word stays, the credential goes.
            if word.eq_ignore_ascii_case("bearer") {
                words.push(word.to_string());
            } else {
                words.push(REDACTED.to_string());
                redact_next = false;
            }
            continue;
        }
        match word.find(['=', ':']) {
            Some(at) if is_sensitive(&word[..at]) => {
                if at + 1 < word.len() {
                    words.push(format!("{}{REDACTED}", &word[..=at]));
                } else {
                    words.push(word.to_string());
                    redact_next = true;
                }
            }
            _ => words.push(word.to_string()),
        }
    }

    let joined = words.join(" ");
    if joined.chars().count() <= EXCERPT_CHARS {
        return joined;
    }
    let cut: String = joined.chars().take(EXCERPT_CHARS).collect();
    format!("{cut}...")
}

fn is_sensitive(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SENSITIVE_KEYS
        .iter()
        .any(|sensitive| key.contains(sensitive))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_keeps_only_the_last_bytes() {
        let mut tail = OutputTail::default();
        tail.push(&[b'a'; TAIL_BYTES * 3]);
        tail.push(b"END");
        let kept: Vec<u8> = tail.bytes.iter().copied().collect();
        assert_eq!(kept.len(), TAIL_BYTES);
        assert!(kept.ends_with(b"END"));
    }

    #[test]
    fn known_secrets_never_reach_the_excerpt() {
        let text = "token abc123 y hook xyz123 y el usuario secreto-de-prueba";
        let clean = redact(text, &["abc123", "xyz123", "secreto-de-prueba"]);
        assert!(!clean.contains("abc123"));
        assert!(!clean.contains("xyz123"));
        assert!(!clean.contains("secreto-de-prueba"));
        assert!(clean.contains(REDACTED));
    }

    #[test]
    fn sensitive_key_values_are_redacted_by_shape() {
        let text = "HN_TOOL_TOKEN=abc Authorization: Bearer zzz \"password\": \"pw\" hora 12:30";
        let clean = redact(text, &[]);
        for leaked in ["abc", "zzz", "\"pw\""] {
            assert!(!clean.contains(leaked), "leaked {leaked}: {clean}");
        }
        assert!(clean.contains("12:30"), "{clean}");
    }

    #[test]
    fn control_characters_are_folded_to_one_line() {
        let clean = redact("uno\u{1b}[31m dos\r\ntres\tcuatro", &[]);
        assert!(!clean.chars().any(char::is_control), "{clean:?}");
        assert!(!clean.contains('\n'));
    }

    #[test]
    fn the_excerpt_is_capped() {
        let clean = redact(&"x ".repeat(EXCERPT_CHARS * 2), &[]);
        assert!(
            clean.chars().count() <= EXCERPT_CHARS + 3,
            "{}",
            clean.len()
        );
    }
}
