//! What a CLI's blocking hook asks the operator about (CORE10B): the tool the
//! CLI is about to run, mapped to the approval kind the policy classifies and
//! to the paths the action names. The hook only sends what this table says; the
//! policy decides from the kind and the paths (inside the project or outside).

use serde_json::Value;

/// The tools the CLI sends to the hook before they run. Claude matches tool
/// names with this pattern; every tool of [`of`] must be named here.
pub const CLAUDE_HOOK_MATCHER: &str = "Bash|Write|Edit|MultiEdit|NotebookEdit|WebFetch";

/// The approval kind and the paths of one tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookAction {
    pub kind: &'static str,
    pub paths: Vec<String>,
}

/// The action of a tool call, or `None` for a tool this table does not know:
/// the hook then keeps its default (a command) and the policy still decides.
pub fn of(tool: &str, input: &Value) -> Option<HookAction> {
    let action = match tool.to_ascii_lowercase().as_str() {
        "bash" => HookAction {
            kind: "command",
            paths: Vec::new(),
        },
        "write" | "edit" | "multiedit" => HookAction {
            kind: "edit",
            paths: named(input, "file_path"),
        },
        "notebookedit" => HookAction {
            kind: "edit",
            paths: named(input, "notebook_path"),
        },
        "webfetch" => HookAction {
            kind: "network",
            paths: Vec::new(),
        },
        _ => return None,
    };
    Some(action)
}

fn named(input: &Value, field: &str) -> Vec<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(|path| vec![path.to_string()])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_gated_tool_maps_to_its_kind_and_its_paths() {
        let cases: Vec<(&str, Value, &str, Vec<&str>)> = vec![
            ("Bash", json!({"command": "ls"}), "command", vec![]),
            (
                "Write",
                json!({"file_path": "src/a.rs", "content": "x"}),
                "edit",
                vec!["src/a.rs"],
            ),
            (
                "Edit",
                json!({"file_path": "../secret.env"}),
                "edit",
                vec!["../secret.env"],
            ),
            (
                "MultiEdit",
                json!({"file_path": "b.rs"}),
                "edit",
                vec!["b.rs"],
            ),
            (
                "NotebookEdit",
                json!({"notebook_path": "n.ipynb"}),
                "edit",
                vec!["n.ipynb"],
            ),
            (
                "WebFetch",
                json!({"url": "https://example.com"}),
                "network",
                vec![],
            ),
        ];
        for (tool, input, kind, paths) in cases {
            let action = of(tool, &input).expect("a gated tool maps");
            let got: Vec<&str> = action.paths.iter().map(String::as_str).collect();
            assert_eq!(action.kind, kind, "{tool}");
            assert_eq!(got, paths, "{tool}");
        }
    }

    #[test]
    fn a_tool_outside_the_table_keeps_the_default() {
        assert_eq!(of("Glob", &json!({"pattern": "*"})), None);
    }

    #[test]
    fn the_matcher_names_every_tool_of_the_table() {
        for tool in [
            "Bash",
            "Write",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "WebFetch",
        ] {
            assert!(
                CLAUDE_HOOK_MATCHER.split('|').any(|name| name == tool),
                "{tool} is not in the matcher"
            );
            assert!(of(tool, &json!({})).is_some(), "{tool} is not mapped");
        }
    }
}
