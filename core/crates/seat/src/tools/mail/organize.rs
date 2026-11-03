//! The organize tools (F2b2): `mail.move_propose` asks the operator to approve a batch
//! of moves, `mail.move_apply` moves the messages once the batch is approved, and
//! `mail.undo_list` lists the recent moves that can still be undone, and `mail.undo` puts
//! one moved message back. Each takes its named fields and nothing else. The destinations
//! are the three folders of [`MoveFolder`], and nothing else can be a destination.

use std::collections::HashSet;

use hn_core_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, invalid, MailCall, MAX_ID_CHARS};

const MAX_BATCH: usize = 20;
const MAX_REASON_CHARS: usize = 200;
const DEFAULT_UNDO_LIST: i64 = 10;
const MAX_UNDO_LIST: i64 = 20;

/// Where a proposed move sends a message. Closed on purpose: these three folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveFolder {
    Junk,
    Deleted,
    Inbox,
}

impl MoveFolder {
    /// The Graph well-known name of the folder.
    pub fn graph_name(self) -> &'static str {
        match self {
            Self::Junk => "junkemail",
            Self::Deleted => "deleteditems",
            Self::Inbox => "inbox",
        }
    }
}

/// A typed organize call. The host turns it into a proposal, an apply or an undo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Organize {
    Propose(MoveProposal),
    Apply(MoveApply),
    Undo(MoveUndo),
    UndoList(UndoList),
}

/// A batch of 1..=20 moves of one account, each message at most once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveProposal {
    pub account: String,
    pub items: Vec<ProposedMove>,
}

/// One message of a batch, where it goes and why Alfred proposes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedMove {
    pub message_id: String,
    pub folder: MoveFolder,
    pub reason: String,
}

/// The batch to apply. Without an id the host finds the approved batch of the account
/// itself: an id does not survive from one turn to the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveApply {
    pub account: String,
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveUndo {
    pub account: String,
    pub action_id: String,
}

/// The newest moves of `account` that can still be undone, at most `n` of them (1..=20).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoList {
    pub account: String,
    pub n: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeArgs {
    account: String,
    items: Vec<ItemArgs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemArgs {
    message_id: String,
    dest: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyArgs {
    account: String,
    approval_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UndoArgs {
    account: String,
    action_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UndoListArgs {
    account: String,
    n: Option<i64>,
}

/// Parses one of the three organize tools. The caller routes only those names here.
pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let organize = match request.tool.as_str() {
        "mail.move_propose" => Organize::Propose(propose(request)?),
        "mail.move_apply" => {
            let args: ApplyArgs = arguments(request)?;
            Organize::Apply(MoveApply {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                approval_id: args
                    .approval_id
                    .map(|id| bounded("approval_id", &id, MAX_ID_CHARS))
                    .transpose()?,
            })
        }
        "mail.undo" => {
            let args: UndoArgs = arguments(request)?;
            Organize::Undo(MoveUndo {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                action_id: bounded("action_id", &args.action_id, MAX_ID_CHARS)?,
            })
        }
        "mail.undo_list" => {
            let args: UndoListArgs = arguments(request)?;
            Organize::UndoList(UndoList {
                account: bounded("account", &args.account, MAX_ID_CHARS)?,
                n: undo_list_count(args.n)?,
            })
        }
        other => {
            return Err(invalid(
                "tool",
                format!("{other:?} is not an organize tool"),
            ))
        }
    };
    Ok(MailCall::Organize(organize))
}

fn propose(request: &ToolRequest) -> Result<MoveProposal, ToolError> {
    let args: ProposeArgs = arguments(request)?;
    if !(1..=MAX_BATCH).contains(&args.items.len()) {
        return Err(invalid(
            "items",
            format!("items must hold 1..={MAX_BATCH} moves"),
        ));
    }
    let items = args
        .items
        .iter()
        .map(proposed)
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = HashSet::new();
    if !items
        .iter()
        .all(|item| ids.insert(item.message_id.as_str()))
    {
        return Err(invalid(
            "items",
            "a message appears twice in the batch".into(),
        ));
    }
    Ok(MoveProposal {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
        items,
    })
}

fn proposed(raw: &ItemArgs) -> Result<ProposedMove, ToolError> {
    Ok(ProposedMove {
        message_id: bounded("message_id", &raw.message_id, MAX_ID_CHARS)?,
        folder: folder(&raw.dest)?,
        reason: reason(&raw.reason)?,
    })
}

fn folder(raw: &str) -> Result<MoveFolder, ToolError> {
    match raw {
        "junkemail" => Ok(MoveFolder::Junk),
        "deleteditems" => Ok(MoveFolder::Deleted),
        "inbox" => Ok(MoveFolder::Inbox),
        _ => Err(invalid(
            "dest",
            "dest must be junkemail, deleteditems or inbox".into(),
        )),
    }
}

/// How many moves to list: 1..=20, and 10 when the call names none.
fn undo_list_count(raw: Option<i64>) -> Result<u32, ToolError> {
    match raw.unwrap_or(DEFAULT_UNDO_LIST) {
        n if (1..=MAX_UNDO_LIST).contains(&n) => Ok(n as u32),
        _ => Err(invalid("n", format!("n must be 1..={MAX_UNDO_LIST}"))),
    }
}

/// The reason is one line the operator reads before deciding.
fn reason(raw: &str) -> Result<String, ToolError> {
    let line = raw.trim();
    if line.is_empty()
        || line.chars().count() > MAX_REASON_CHARS
        || line.chars().any(char::is_control)
    {
        return Err(invalid(
            "reason",
            format!("reason must be 1..={MAX_REASON_CHARS} characters on one line"),
        ));
    }
    Ok(line.to_string())
}

#[cfg(test)]
mod tests {
    use hn_core_chat::{ToolError, ToolRequest};
    use serde_json::{json, Value};

    use super::{
        parse, MailCall, MoveApply, MoveFolder, MoveProposal, Organize, ProposedMove, UndoList,
    };

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    fn item(id: &str, dest: &str) -> Value {
        json!({"message_id": id, "dest": dest, "reason": "boletín sin suscripción"})
    }

    fn refused(tool: &str, arguments: Value) -> ToolError {
        parse(&request(tool, arguments)).expect_err("refused")
    }

    #[test]
    fn a_batch_keeps_its_order_and_its_destinations() {
        let parsed = parse(&request(
            "mail.move_propose",
            json!({"account": "trabajo", "items": [item("m1", "junkemail"), item("m2", "inbox")]}),
        ));
        assert_eq!(
            parsed,
            Ok(MailCall::Organize(Organize::Propose(MoveProposal {
                account: "trabajo".into(),
                items: vec![
                    ProposedMove {
                        message_id: "m1".into(),
                        folder: MoveFolder::Junk,
                        reason: "boletín sin suscripción".into(),
                    },
                    ProposedMove {
                        message_id: "m2".into(),
                        folder: MoveFolder::Inbox,
                        reason: "boletín sin suscripción".into(),
                    },
                ],
            })))
        );
    }

    #[test]
    fn a_batch_holds_one_to_twenty_distinct_messages() {
        let none = json!({"account": "t", "items": []});
        assert_eq!(
            refused("mail.move_propose", none).code,
            "mail_invalid_input"
        );
        let many: Vec<Value> = (0..21).map(|n| item(&format!("m{n}"), "inbox")).collect();
        let too_many = json!({"account": "t", "items": many});
        assert_eq!(refused("mail.move_propose", too_many).field, Some("items"));
        let repeated =
            json!({"account": "t", "items": [item("m1", "inbox"), item("m1", "junkemail")]});
        assert_eq!(refused("mail.move_propose", repeated).field, Some("items"));
    }

    #[test]
    fn a_destination_outside_the_three_folders_is_refused() {
        for dest in ["sentitems", "archive", "AAMkAD-id-de-carpeta", "trash"] {
            let arguments = json!({"account": "t", "items": [item("m1", dest)]});
            assert_eq!(
                refused("mail.move_propose", arguments).field,
                Some("dest"),
                "{dest}"
            );
        }
    }

    #[test]
    fn a_reason_is_one_line_of_at_most_200_characters() {
        for reason in ["", "   ", "uno\ndos", &"x".repeat(201)] {
            let arguments = json!({"account": "t", "items": [{
                "message_id": "m1", "dest": "inbox", "reason": reason
            }]});
            assert_eq!(
                refused("mail.move_propose", arguments).field,
                Some("reason")
            );
        }
    }

    #[test]
    fn apply_and_undo_take_their_named_fields_and_nothing_else() {
        assert!(matches!(
            parse(&request(
                "mail.move_apply",
                json!({"account": "t", "approval_id": "apr_1"})
            )),
            Ok(MailCall::Organize(Organize::Apply(_)))
        ));
        assert_eq!(
            parse(&request("mail.move_apply", json!({"account": "t"}))),
            Ok(MailCall::Organize(Organize::Apply(MoveApply {
                account: "t".into(),
                approval_id: None,
            })))
        );
        assert!(matches!(
            parse(&request(
                "mail.undo",
                json!({"account": "t", "action_id": "01H"})
            )),
            Ok(MailCall::Organize(Organize::Undo(_)))
        ));
        assert_eq!(
            refused(
                "mail.undo",
                json!({"account": "t", "action_id": "01H", "dest": "inbox"})
            )
            .code,
            "mail_invalid_input"
        );
    }

    #[test]
    fn undo_list_counts_one_to_twenty_and_defaults_to_ten() {
        let listed = |arguments: Value| parse(&request("mail.undo_list", arguments));
        assert_eq!(
            listed(json!({"account": "t"})),
            Ok(MailCall::Organize(Organize::UndoList(UndoList {
                account: "t".into(),
                n: 10,
            })))
        );
        assert_eq!(
            listed(json!({"account": "t", "n": 20})),
            Ok(MailCall::Organize(Organize::UndoList(UndoList {
                account: "t".into(),
                n: 20,
            })))
        );
        for n in [json!(0), json!(21), json!(-1)] {
            let arguments = json!({"account": "t", "n": n});
            assert_eq!(refused("mail.undo_list", arguments).field, Some("n"), "{n}");
        }
        let text = json!({"account": "t", "n": "5"});
        assert_eq!(refused("mail.undo_list", text).code, "mail_invalid_input");
        let extra = json!({"account": "t", "subject": "x"});
        assert_eq!(refused("mail.undo_list", extra).code, "mail_invalid_input");
        assert_eq!(
            refused("mail.undo_list", json!({})).code,
            "mail_invalid_input"
        );
    }
}
