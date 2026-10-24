//! Request validation with the contract's stable action codes
//! (`docs/contracts/errors.md`). One place per rule, so every route answers
//! the same way.

use crate::error::ApiError;

/// Pane ids look like `w1:p3`. Anything else is not a pane this host knows,
/// and passing an arbitrary string through would let a caller address agents
/// by name instead.
pub fn validate_pane_id(pane_id: &str) -> Result<(), ApiError> {
    let invalid = || ApiError::action("bad_pane", format!("{pane_id:?} is not a pane id"));
    let Some((workspace, pane)) = pane_id.split_once(":p") else {
        return Err(invalid());
    };
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    };
    if valid(workspace) && valid(pane) {
        Ok(())
    } else {
        Err(invalid())
    }
}

/// Content-check a submitted prompt. Newlines are legitimate (multi-line
/// instructions); the characters that would type something other than text
/// into the agent's terminal are refused.
pub fn validate_prompt(text: &str) -> Result<String, ApiError> {
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return Err(ApiError::action("empty_prompt", "text is required"));
    }
    if cleaned
        .chars()
        .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return Err(ApiError::action(
            "control_characters",
            "prompt contains control characters",
        ));
    }
    Ok(cleaned.to_string())
}

/// Keys a caller may send without further ceremony: navigation and the
/// answers to the yes/no and numbered dialogs agents actually present.
const SAFE_KEYS: &[&str] = &[
    "up",
    "down",
    "left",
    "right",
    "enter",
    "esc",
    "escape",
    "tab",
    "space",
    "backspace",
    "home",
    "end",
    "pgup",
    "pgdown",
    "y",
    "n",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
];

/// Keys that change or destroy work, and therefore need explicit confirmation.
const GUARDED_KEYS: &[&str] = &["ctrl+c"];

/// Normalise and allowlist the keys a caller wants to send.
pub fn validate_keys(keys: &[String], confirm: bool) -> Result<Vec<String>, ApiError> {
    if keys.is_empty() {
        return Err(ApiError::action("no_keys", "at least one key is required"));
    }
    if keys.len() > 8 {
        return Err(ApiError::action(
            "too_many_keys",
            "at most 8 keys per request",
        ));
    }
    let mut normalised = Vec::with_capacity(keys.len());
    for key in keys {
        let candidate = key.trim().to_ascii_lowercase();
        if candidate.is_empty() {
            return Err(ApiError::action("empty_key", "keys must not be empty"));
        }
        if GUARDED_KEYS.contains(&candidate.as_str()) && !confirm {
            return Err(ApiError::action(
                "confirmation_required",
                format!("{candidate} changes running work and requires confirm=true"),
            ));
        }
        if !SAFE_KEYS.contains(&candidate.as_str()) && !GUARDED_KEYS.contains(&candidate.as_str()) {
            return Err(ApiError::action(
                "key_not_allowed",
                format!("{candidate:?} is not an allowed key"),
            ));
        }
        normalised.push(if candidate == "escape" {
            "esc".to_string()
        } else {
            candidate
        });
    }
    Ok(normalised)
}

/// Clamp a read request to 1..=200, default 60 (contract `api.md`).
pub fn clamp_read_lines(lines: Option<i64>) -> u16 {
    match lines {
        None => 60,
        Some(lines) => lines.clamp(1, 200) as u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(error: ApiError) -> String {
        error.body["detail"]["code"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn pane_ids_follow_the_contract_regex() {
        assert!(validate_pane_id("w1:p3").is_ok());
        assert!(validate_pane_id("wQ:p3Q").is_ok());
        assert_eq!(code(validate_pane_id("claude").unwrap_err()), "bad_pane");
        assert_eq!(code(validate_pane_id("w1:3").unwrap_err()), "bad_pane");
        assert_eq!(code(validate_pane_id("w-1:p3").unwrap_err()), "bad_pane");
        assert_eq!(code(validate_pane_id("").unwrap_err()), "bad_pane");
    }

    #[test]
    fn prompts_are_trimmed_and_control_characters_refused() {
        assert_eq!(validate_prompt("  hola  ").unwrap(), "hola");
        assert!(validate_prompt("linea 1\nlinea 2\ttab").is_ok());
        assert_eq!(code(validate_prompt("   ").unwrap_err()), "empty_prompt");
        assert_eq!(
            code(validate_prompt("bell\x07").unwrap_err()),
            "control_characters"
        );
        assert_eq!(
            code(validate_prompt("esc\x1b[1m").unwrap_err()),
            "control_characters"
        );
    }

    #[test]
    fn keys_are_allowlisted_and_normalised() {
        let keys = vec![" ESC ".to_string(), "Enter".to_string()];
        assert_eq!(validate_keys(&keys, false).unwrap(), vec!["esc", "enter"]);
        assert_eq!(
            code(validate_keys(&["ctrl+c".to_string()], false).unwrap_err()),
            "confirmation_required"
        );
        assert!(validate_keys(&["ctrl+c".to_string()], true).is_ok());
        assert_eq!(
            code(validate_keys(&["f5".to_string()], false).unwrap_err()),
            "key_not_allowed"
        );
        assert_eq!(code(validate_keys(&[], false).unwrap_err()), "no_keys");
        assert_eq!(
            code(validate_keys(&vec!["y".to_string(); 9], false).unwrap_err()),
            "too_many_keys"
        );
        assert_eq!(
            code(validate_keys(&["".to_string()], false).unwrap_err()),
            "empty_key"
        );
    }

    #[test]
    fn read_lines_are_clamped() {
        assert_eq!(clamp_read_lines(None), 60);
        assert_eq!(clamp_read_lines(Some(0)), 1);
        assert_eq!(clamp_read_lines(Some(500)), 200);
        assert_eq!(clamp_read_lines(Some(40)), 40);
    }
}
