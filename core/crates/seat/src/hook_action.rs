//! What a CLI's blocking hook asks the operator about (CORE10B): the tool the
//! CLI is about to run, mapped to the approval kind the policy classifies and
//! to the paths the action names. The hook only sends what this table says; the
//! policy decides from the kind and the paths (inside the project or outside).

use serde_json::Value;

/// The tools the CLI sends to the hook before they run. Claude matches tool
/// names with this pattern; every tool of [`of`] must be named here.
pub const CLAUDE_HOOK_MATCHER: &str =
    "Bash|PowerShell|Write|Edit|MultiEdit|NotebookEdit|WebFetch|WebSearch";

// shortcut: the card cuts a search query at 200 characters (the API refuses a summary over
// 400); the full query stays in the payload. Upgrade if cards must show every word.
const SEARCH_CARD_CHARS: usize = 200;

/// The approval kind and the paths of one tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookAction {
    pub kind: &'static str,
    pub paths: Vec<String>,
    /// The card's line when the call names none of its own.
    pub summary: Option<String>,
}

/// The action of a tool call, or `None` for a tool this table does not know:
/// the hook then keeps its default (a command) and the policy still decides.
pub fn of(tool: &str, input: &Value) -> Option<HookAction> {
    let action = match tool.to_ascii_lowercase().as_str() {
        "bash" | "powershell" => HookAction {
            kind: "command",
            paths: Vec::new(),
            summary: None,
        },
        "write" | "edit" | "multiedit" => HookAction {
            kind: "edit",
            paths: named(input, "file_path"),
            summary: None,
        },
        "notebookedit" => HookAction {
            kind: "edit",
            paths: named(input, "notebook_path"),
            summary: None,
        },
        "webfetch" => HookAction {
            kind: "network",
            paths: Vec::new(),
            summary: None,
        },
        "websearch" => HookAction {
            kind: "network",
            paths: Vec::new(),
            summary: Some(format!(
                "Buscar en internet: {}",
                text(input, "query")
                    .chars()
                    .take(SEARCH_CARD_CHARS)
                    .collect::<String>()
            )),
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

fn text(input: &Value, field: &str) -> String {
    input
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
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
                "PowerShell",
                json!({"command": "Remove-Item README.md"}),
                "command",
                vec![],
            ),
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
            (
                "WebSearch",
                json!({"query": "precio del café"}),
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
    fn a_web_search_card_names_its_query_and_cuts_a_long_one() {
        let short = of("WebSearch", &json!({"query": "precio del café"})).expect("maps");
        assert_eq!(
            short.summary.as_deref(),
            Some("Buscar en internet: precio del café")
        );
        let long = "a".repeat(SEARCH_CARD_CHARS + 50);
        let cut = of("WebSearch", &json!({ "query": long })).expect("maps");
        let summary = cut.summary.expect("summary");
        assert_eq!(
            summary.chars().count(),
            "Buscar en internet: ".chars().count() + SEARCH_CARD_CHARS
        );
    }

    #[test]
    fn a_tool_outside_the_table_keeps_the_default() {
        assert_eq!(of("Glob", &json!({"pattern": "*"})), None);
    }

    #[test]
    fn the_matcher_names_every_tool_of_the_table() {
        for tool in [
            "Bash",
            "PowerShell",
            "Write",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
        ] {
            assert!(
                CLAUDE_HOOK_MATCHER.split('|').any(|name| name == tool),
                "{tool} is not in the matcher"
            );
            assert!(of(tool, &json!({})).is_some(), "{tool} is not mapped");
        }
    }
}
