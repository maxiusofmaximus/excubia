//! The canonical digest of an action.
//!
//! `docs/rfc/orchestrator-chat.md` §5 fixes the shape: `sha256:` plus the
//! SHA-256 of the stable JSON of `{kind, tool, args, cwd, paths}`. The OpenCode
//! plugin computes the same digest in JavaScript
//! (`tooling/opencode-uac-plugin/src/digest.js`), so the rules are mirrored
//! exactly here: object keys sorted, arrays in order, `paths` sorted, and the
//! same depth and item bounds.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The plugin's `MAX_DEPTH`: a value deeper than this is replaced.
const MAX_DEPTH: usize = 6;
/// The plugin's `MAX_ITEMS`: a longer array is truncated.
const MAX_ITEMS: usize = 50;
/// The plugin's marker for a truncated value.
const TRUNCATED: &str = "[truncado]";

/// The stable JSON of a value: sorted keys, bounded depth and length.
pub fn canonical_json(value: &Value) -> String {
    serde_json::to_string(&stable(value, 0)).unwrap_or_else(|_| "{}".to_string())
}

/// The digest of one concrete action. Two requests with the same action
/// produce the same digest, and the host refuses the second one: that is the
/// replay protection of the RFC.
pub fn action_digest(kind: &str, tool: &str, args: &Value, cwd: &str, paths: &[String]) -> String {
    let mut sorted: Vec<String> = paths.to_vec();
    sorted.sort();
    let canonical = canonical_json(&json!({
        "kind": kind,
        "tool": tool,
        "args": args,
        "cwd": cwd,
        "paths": sorted,
    }));
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    )
}

/// A digest is well formed when it is `sha256:` plus 64 hexadecimal digits.
pub fn is_well_formed(digest: &str) -> bool {
    let Some(hex_part) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex_part.len() == 64 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
}

fn stable(value: &Value, depth: usize) -> Value {
    match value {
        Value::Array(items) => {
            if depth >= MAX_DEPTH {
                return Value::String(TRUNCATED.to_string());
            }
            Value::Array(
                items
                    .iter()
                    .take(MAX_ITEMS)
                    .map(|item| stable(item, depth + 1))
                    .collect(),
            )
        }
        Value::Object(map) => {
            if depth >= MAX_DEPTH {
                return Value::String(TRUNCATED.to_string());
            }
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), stable(&map[key], depth + 1));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_json_sorts_the_keys() {
        let value = json!({"b": 1, "a": {"z": true, "y": [3, 2, 1]}});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"y":[3,2,1],"z":true},"b":1}"#
        );
    }

    #[test]
    fn the_digest_is_stable_and_ignores_the_key_order() {
        let first = action_digest(
            "command",
            "bash",
            &json!({"command": "cargo test", "flag": true}),
            "/work",
            &[],
        );
        let second = action_digest(
            "command",
            "bash",
            &json!({"flag": true, "command": "cargo test"}),
            "/work",
            &[],
        );
        assert_eq!(first, second);
        assert!(is_well_formed(&first), "{first}");
    }

    #[test]
    fn the_paths_are_sorted_and_the_rest_is_not() {
        let mut first = vec!["/b".to_string(), "/a".to_string()];
        first.sort();
        let sorted = action_digest("edit", "write", &json!({}), "/work", &first);
        let unsorted = action_digest(
            "edit",
            "write",
            &json!({}),
            "/work",
            &["/b".to_string(), "/a".to_string()],
        );
        assert_eq!(sorted, unsorted);
        let other = action_digest("edit", "write", &json!({}), "/work", &["/c".to_string()]);
        assert_ne!(sorted, other);
    }

    #[test]
    fn a_malformed_digest_is_refused() {
        assert!(!is_well_formed("sha256:zz"));
        assert!(!is_well_formed(""));
        assert!(!is_well_formed(
            "md5:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
    }
}
