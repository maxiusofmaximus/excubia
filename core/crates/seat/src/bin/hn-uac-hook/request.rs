//! What the agent wants to do, read from the command line and from the CLI's
//! payload on stdin.

use std::io::Read;

use hn_core_seat::hook_action;
use serde_json::{json, Value};

use super::Refusal;

/// What the agent wants to do.
pub(super) struct Request {
    pub(super) kind: String,
    pub(super) tool: String,
    pub(super) summary: String,
    pub(super) args: Value,
    pub(super) cwd: String,
    pub(super) paths: Vec<String>,
    pub(super) ttl_seconds: u64,
}

impl Request {
    /// The arguments win over the payload of the CLI: an explicit flag is
    /// what the operator configured, the payload is what the CLI guessed.
    pub(super) fn read() -> Result<Self, Refusal> {
        let flags = Flags::parse(std::env::args().skip(1))?;
        let payload = if flags.tool.is_none() {
            read_payload()?
        } else {
            None
        };
        let tool = flags
            .tool
            .or_else(|| payload.as_ref().and_then(|p| p.tool_name.clone()))
            .ok_or_else(|| {
                Refusal::Usage("the hook needs a tool (--tool or a payload on stdin)".to_string())
            })?;
        let args = flags
            .args
            .or_else(|| payload.as_ref().and_then(|p| p.args.clone()))
            .unwrap_or_else(|| json!({}));
        // The tool table says what the call is (an edit names its file); a tool
        // it does not know keeps the old default, a command.
        let mapped = hook_action::of(&tool, &args);
        let kind = flags
            .kind
            .or_else(|| payload.as_ref().and_then(|p| p.kind.clone()))
            .or_else(|| mapped.as_ref().map(|action| action.kind.to_string()))
            .unwrap_or_else(|| "command".to_string());
        let summary = flags
            .summary
            .or_else(|| payload.as_ref().and_then(|p| p.summary.clone()))
            .unwrap_or_else(|| format!("{kind} {tool}"));
        let cwd = flags
            .cwd
            .or_else(|| payload.as_ref().and_then(|p| p.cwd.clone()))
            .unwrap_or_default();
        let paths = match (flags.paths.is_empty(), mapped) {
            (true, Some(action)) => action.paths,
            _ => flags.paths,
        };
        Ok(Self {
            kind,
            tool,
            summary,
            args,
            cwd,
            paths,
            ttl_seconds: flags.ttl_seconds,
        })
    }
}

/// The command line. Every problem here is a usage error: the hook never
/// guesses what the operator meant.
#[derive(Debug)]
struct Flags {
    kind: Option<String>,
    tool: Option<String>,
    summary: Option<String>,
    args: Option<Value>,
    cwd: Option<String>,
    paths: Vec<String>,
    ttl_seconds: u64,
}

impl Flags {
    fn parse(argv: impl Iterator<Item = String>) -> Result<Self, Refusal> {
        let argv: Vec<String> = argv.collect();
        let mut flags = Self {
            kind: None,
            tool: None,
            summary: None,
            args: None,
            cwd: None,
            paths: Vec::new(),
            // The longest the hook waits when the payload does not say.
            ttl_seconds: u64::from(hn_core_chat::DEFAULT_TTL_SECONDS),
        };
        let mut index = 0;
        while index < argv.len() {
            let argument = argv[index].clone();
            index += 1;
            let mut value = || -> Result<String, Refusal> {
                let value = argv
                    .get(index)
                    .cloned()
                    .ok_or_else(|| Refusal::Usage(format!("{argument} needs a value")))?;
                index += 1;
                Ok(value)
            };
            match argument.as_str() {
                "--kind" => flags.kind = Some(value()?),
                "--tool" => flags.tool = Some(value()?),
                "--summary" => flags.summary = Some(value()?),
                "--args" => {
                    let raw = value()?;
                    flags.args =
                        Some(serde_json::from_str(&raw).map_err(|error| {
                            Refusal::Usage(format!("--args is not JSON: {error}"))
                        })?);
                }
                "--cwd" => flags.cwd = Some(value()?),
                "--paths" => {
                    flags.paths = value()?
                        .split(',')
                        .map(|path| path.trim().to_string())
                        .filter(|path| !path.is_empty())
                        .collect();
                }
                "--ttl" => {
                    let raw = value()?;
                    flags.ttl_seconds = raw.parse().map_err(|_| {
                        Refusal::Usage("--ttl needs a number of seconds".to_string())
                    })?;
                }
                other => {
                    return Err(Refusal::Usage(format!("unknown argument: {other}")));
                }
            }
        }
        Ok(flags)
    }
}

/// The payload a CLI hands to its hook on stdin.
struct Payload {
    tool_name: Option<String>,
    kind: Option<String>,
    summary: Option<String>,
    args: Option<Value>,
    cwd: Option<String>,
}

fn read_payload() -> Result<Option<Payload>, Refusal> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|error| Refusal::NoDecision(format!("cannot read the hook payload: {error}")))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| Refusal::NoDecision(format!("the hook payload is not JSON: {error}")))?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.to_string())
    };
    Ok(Some(Payload {
        tool_name: text("tool_name").or_else(|| text("tool")),
        kind: text("kind"),
        summary: text("summary"),
        args: value
            .get("tool_input")
            .cloned()
            .or_else(|| value.get("args").cloned()),
        cwd: text("cwd"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Flags, Refusal> {
        Flags::parse(arguments.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn an_unknown_argument_is_a_usage_error() {
        let error = parse(&["--tool", "bash", "--nope"]).expect_err("unknown flag");
        assert!(matches!(error, Refusal::Usage(_)));
    }

    #[test]
    fn a_missing_value_is_a_usage_error() {
        let error = parse(&["--tool"]).expect_err("missing value");
        assert!(matches!(error, Refusal::Usage(_)));
    }

    #[test]
    fn the_arguments_become_a_request() {
        let flags = parse(&[
            "--kind", "command", "--tool", "bash", "--ttl", "5", "--paths", "a, b",
        ])
        .expect("flags");
        assert_eq!(flags.kind.as_deref(), Some("command"));
        assert_eq!(flags.tool.as_deref(), Some("bash"));
        assert_eq!(flags.ttl_seconds, 5);
        assert_eq!(flags.paths, vec!["a".to_string(), "b".to_string()]);
    }
}
