//! Spotting a turn that ran out of Claude's usage quota, from the CLI's own output. Overload
//! (`overloaded_error`, 529) is saturation, not quota, and is not counted.

/// Phrases the CLI prints when the account's quota is spent. Matched in lower case.
const PHRASES: &[&str] = &[
    "usage limit",
    "rate limit",
    "limit reached",
    "limit will reset",
    "credit balance is too low",
    "quota",
    "out of extra usage",
];

/// Whether a failed turn's output says the account's quota is spent. The CLI's text is not
/// fixed, so the match is tolerant: case-insensitive phrases, and `429` as a number of its own.
pub fn looks_like_quota_exhausted(output: &str) -> bool {
    let lower = output.to_lowercase();
    PHRASES.iter().any(|phrase| lower.contains(*phrase)) || has_status_429(&lower)
}

/// `429` as a number of its own, so a port or a count like `1429` does not match.
fn has_status_429(text: &str) -> bool {
    text.match_indices("429").any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + 3..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::looks_like_quota_exhausted as quota;

    #[test]
    fn typical_quota_messages_are_quota() {
        for text in [
            "Claude AI usage limit reached|1730000000",
            "Error: rate limit exceeded, try again later",
            "You're out of extra usage · resets 5pm",
            "Your credit balance is too low to access the Anthropic API",
            "API Error: 429 Too Many Requests",
            "monthly quota exhausted",
            "Claude usage limit reached. Your limit will reset at 5pm.",
        ] {
            assert!(quota(text), "not seen as quota: {text}");
        }
    }

    #[test]
    fn overload_exit_and_plain_text_are_not_quota() {
        for text in [
            "",
            "process_exit: 1",
            "overloaded_error",
            "API Error: 529 {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}",
            "I can limit the results to ten rows.",
            "limit the scope of the search",
            "The port 1429 is busy.",
        ] {
            assert!(!quota(text), "seen as quota: {text}");
        }
    }
}
