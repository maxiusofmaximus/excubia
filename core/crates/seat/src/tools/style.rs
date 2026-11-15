//! `style.get` (ESTILO0B, RFC estilo-aprendido §4.1): Alfred reads the operator's writing
//! rules before a draft. It takes no arguments, so a caller cannot pick another file,
//! and it changes nothing. `style.learn_sent` (F1, §2.1) reads the statistics of the
//! operator's sent mail: an account and a count of 1..=100, nothing else.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::parse::{arguments, name_field};
use super::{no_arguments, ToolCall};

const DEFAULT_LEARN_LIMIT: i64 = 100;
const MAX_LEARN_LIMIT: i64 = 100;

/// `style.learn_sent`: the account whose sent mail is read, and how many messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleLearnQuery {
    pub account: String,
    pub limit: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LearnArgs {
    account: String,
    #[serde(default)]
    limit: Option<i64>,
}

pub fn parse(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    no_arguments(request)?;
    Ok(ToolCall::StyleGet)
}

/// The longest rule line, in characters. Longer text is refused, never cut.
pub const RULE_MAX_CHARS: usize = 300;
const SOURCES: [&str; 3] = ["enviados", "correccion", "manual"];

/// `style.propose_rule`: one rule line and where it came from. Nothing is saved here;
/// the host checks the line and writes it into `estilo.md` at once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleProposal {
    pub rule: String,
    /// `enviados`, `correccion` or `manual`.
    pub source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeArgs {
    rule: String,
    source: String,
}

/// A rule is one non-empty line of at most 300 characters, and the source is one of three.
pub fn parse_propose(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: ProposeArgs = arguments(request, "arguments")?;
    let rule = args.rule.trim().to_owned();
    if rule.is_empty() || rule.chars().count() > RULE_MAX_CHARS || rule.contains(['\n', '\r']) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "rule",
            "rule must be one line of 1..=300 characters",
        ));
    }
    if !SOURCES.contains(&args.source.as_str()) {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "source",
            "source must be enviados, correccion or manual",
        ));
    }
    Ok(ToolCall::StylePropose(StyleProposal {
        rule,
        source: args.source,
    }))
}

/// A limit is refused when it is outside 1..=100; it is never clamped in silence.
pub fn parse_learn(request: &ToolRequest) -> Result<ToolCall, ToolError> {
    let args: LearnArgs = arguments(request, "arguments")?;
    let account = name_field("account", &args.account)?;
    let limit = match args.limit.unwrap_or(DEFAULT_LEARN_LIMIT) {
        limit if (1..=MAX_LEARN_LIMIT).contains(&limit) => limit as u32,
        _ => {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "limit",
                "limit must be 1..=100",
            ))
        }
    };
    Ok(ToolCall::StyleLearn(StyleLearnQuery { account, limit }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    #[test]
    fn the_read_takes_no_arguments() {
        assert_eq!(
            parse(&request("style.get", json!({}))),
            Ok(ToolCall::StyleGet)
        );

        // A path is refused, never applied to the read.
        let widened = parse(&request("style.get", json!({ "path": "C:\\" })));
        assert_eq!(
            widened.expect_err("unknown field").code,
            "invalid_arguments"
        );
    }

    #[test]
    fn learn_takes_an_account_and_a_limit_of_1_to_100() {
        let default = parse_learn(&request(
            "style.learn_sent",
            json!({ "account": "trabajo" }),
        ));
        assert_eq!(
            default,
            Ok(ToolCall::StyleLearn(StyleLearnQuery {
                account: "trabajo".into(),
                limit: 100,
            }))
        );

        let short = parse_learn(&request(
            "style.learn_sent",
            json!({ "account": "trabajo", "limit": 1 }),
        ));
        assert!(matches!(short, Ok(ToolCall::StyleLearn(q)) if q.limit == 1));
    }

    #[test]
    fn a_limit_outside_1_to_100_is_an_argument_error() {
        for limit in [0, 101, -5] {
            let refused = parse_learn(&request(
                "style.learn_sent",
                json!({ "account": "trabajo", "limit": limit }),
            ));
            let error = refused.expect_err("out of range");
            assert_eq!(error.code, "invalid_arguments", "limit {limit}");
            assert_eq!(error.field, Some("limit"));
        }
    }

    #[test]
    fn learn_needs_its_account_and_refuses_other_fields() {
        let no_account = parse_learn(&request("style.learn_sent", json!({ "limit": 5 })));
        assert_eq!(no_account.expect_err("account").code, "invalid_arguments");

        let widened = parse_learn(&request(
            "style.learn_sent",
            json!({ "account": "trabajo", "folder": "inbox" }),
        ));
        assert_eq!(
            widened.expect_err("unknown field").code,
            "invalid_arguments"
        );
    }

    #[test]
    fn propose_takes_one_line_and_a_known_source() {
        let ok = parse_propose(&request(
            "style.propose_rule",
            json!({ "rule": "  Frases cortas.  ", "source": "correccion" }),
        ));
        assert!(matches!(
            ok,
            Ok(ToolCall::StylePropose(StyleProposal { ref rule, ref source }))
                if rule == "Frases cortas." && source == "correccion"
        ));

        let long = "a".repeat(RULE_MAX_CHARS + 1);
        let refused = [
            json!({ "rule": "", "source": "manual" }),
            json!({ "rule": long, "source": "manual" }),
            json!({ "rule": "Dos\nlíneas", "source": "manual" }),
            json!({ "rule": "Frases cortas.", "source": "chat" }),
            json!({ "rule": "Frases cortas." }),
            json!({ "rule": "Frases cortas.", "source": "manual", "to": "x" }),
        ];
        for arguments in refused {
            let error = parse_propose(&request("style.propose_rule", arguments.clone()));
            assert_eq!(
                error.expect_err("refused").code,
                "invalid_arguments",
                "{arguments}"
            );
        }
    }
}
