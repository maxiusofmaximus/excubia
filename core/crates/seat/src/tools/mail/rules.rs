//! The mailbox rule tools (F3B): `mail.rule_list` reads the rules of one account
//! and `mail.rule_create` files one closed rule for the operator to decide. The
//! arguments are exactly the vocabulary of the host's rule type: there is no
//! forwarding, redirecting, deletion or rule-stopping field, and an argument the
//! schema does not name is refused.

use excubia_chat::{ToolError, ToolRequest};
use serde::Deserialize;

use super::{arguments, bounded, invalid, MailCall, MAX_ID_CHARS};

const MAX_NAME_CHARS: usize = 100;
const MAX_CONDITIONS: usize = 10;
const MAX_CONDITION_CHARS: usize = 200;
const MAX_CATEGORIES: usize = 5;
const MAX_CATEGORY_CHARS: usize = 50;
const MAX_FOLDER_CHARS: usize = 200;

/// The listing of one account's inbox rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleListQuery {
    pub account: String,
}

/// One rule this product may create. The fields are the whole vocabulary: no
/// forwarding, redirecting, deletion or rule-stopping action exists here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailRuleDraft {
    pub account: String,
    pub name: String,
    pub conditions: RuleConditionsDraft,
    pub actions: RuleActionsDraft,
}

/// The conditions a rule may have. At least one list must carry an entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleConditionsDraft {
    pub from_addresses: Vec<String>,
    pub sender_contains: Vec<String>,
    pub subject_contains: Vec<String>,
}

/// The actions a rule may have. At least one must be set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleActionsDraft {
    pub move_to_folder: Option<RuleFolder>,
    pub mark_as_read: bool,
    pub assign_categories: Vec<String>,
}

/// Where a rule may move a message: the two safe well-known folders, or a
/// folder of this mailbox by id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleFolder {
    JunkEmail,
    DeletedItems,
    Folder(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    account: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    account: String,
    name: String,
    conditions: ConditionsArgs,
    actions: ActionsArgs,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionsArgs {
    #[serde(default)]
    from_addresses: Vec<String>,
    #[serde(default)]
    sender_contains: Vec<String>,
    #[serde(default)]
    subject_contains: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionsArgs {
    #[serde(default)]
    move_to_folder: Option<String>,
    #[serde(default)]
    mark_as_read: bool,
    #[serde(default)]
    assign_categories: Vec<String>,
}

pub fn parse(request: &ToolRequest) -> Result<MailCall, ToolError> {
    match request.tool.as_str() {
        "mail.rule_list" => list(request),
        "mail.rule_create" => create(request),
        other => Err(invalid(
            "tool",
            format!("{other:?} is not a mailbox rule tool"),
        )),
    }
}

fn list(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let args: ListArgs = arguments(request)?;
    Ok(MailCall::RuleList(RuleListQuery {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
    }))
}

fn create(request: &ToolRequest) -> Result<MailCall, ToolError> {
    let args: CreateArgs = arguments(request)?;
    let conditions = RuleConditionsDraft {
        from_addresses: condition_list("from_addresses", args.conditions.from_addresses)?,
        sender_contains: condition_list("sender_contains", args.conditions.sender_contains)?,
        subject_contains: condition_list("subject_contains", args.conditions.subject_contains)?,
    };
    let actions = RuleActionsDraft {
        move_to_folder: args
            .actions
            .move_to_folder
            .as_deref()
            .map(folder)
            .transpose()?,
        mark_as_read: args.actions.mark_as_read,
        assign_categories: categories(args.actions.assign_categories)?,
    };
    Ok(MailCall::RuleCreate(MailRuleDraft {
        account: bounded("account", &args.account, MAX_ID_CHARS)?,
        name: bounded("name", &args.name, MAX_NAME_CHARS)?,
        conditions,
        actions,
    }))
}

fn condition_list(field: &'static str, values: Vec<String>) -> Result<Vec<String>, ToolError> {
    if values.len() > MAX_CONDITIONS {
        return Err(invalid(
            field,
            format!("{field} holds at most {MAX_CONDITIONS} entries"),
        ));
    }
    values
        .iter()
        .map(|value| bounded(field, value, MAX_CONDITION_CHARS))
        .collect()
}

fn categories(values: Vec<String>) -> Result<Vec<String>, ToolError> {
    if values.len() > MAX_CATEGORIES {
        return Err(invalid(
            "assign_categories",
            format!("assign_categories holds at most {MAX_CATEGORIES} entries"),
        ));
    }
    values
        .iter()
        .map(|value| bounded("assign_categories", value, MAX_CATEGORY_CHARS))
        .collect()
}

fn folder(raw: &str) -> Result<RuleFolder, ToolError> {
    let value = bounded("move_to_folder", raw, MAX_FOLDER_CHARS)?;
    Ok(match value.as_str() {
        "junkemail" => RuleFolder::JunkEmail,
        "deleteditems" => RuleFolder::DeletedItems,
        _ => RuleFolder::Folder(value),
    })
}

#[cfg(test)]
mod tests {
    use excubia_chat::ToolRequest;
    use serde_json::{json, Value};

    use super::{
        MailCall, MailRuleDraft, RuleActionsDraft, RuleConditionsDraft, RuleFolder, RuleListQuery,
    };

    fn request(tool: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            tool: tool.to_string(),
            arguments,
        }
    }

    fn refused(tool: &str, arguments: Value) -> excubia_chat::ToolError {
        super::parse(&request(tool, arguments)).expect_err("refused")
    }

    fn create_args() -> Value {
        json!({
            "account": "trabajo",
            "name": "Facturas",
            "conditions": {
                "from_addresses": ["alguien@example.com"],
                "subject_contains": ["factura"]
            },
            "actions": {
                "move_to_folder": "junkemail",
                "mark_as_read": true,
                "assign_categories": ["facturas"]
            }
        })
    }

    #[test]
    fn the_rule_list_takes_only_the_account() {
        assert_eq!(
            super::parse(&request("mail.rule_list", json!({"account": "trabajo"}))),
            Ok(MailCall::RuleList(RuleListQuery {
                account: "trabajo".into(),
            }))
        );
        assert_eq!(
            refused("mail.rule_list", json!({"account": "trabajo", "limit": 5})).code,
            "mail_invalid_input"
        );
    }

    #[test]
    fn a_rule_carries_exactly_the_closed_vocabulary() {
        assert_eq!(
            super::parse(&request("mail.rule_create", create_args())),
            Ok(MailCall::RuleCreate(MailRuleDraft {
                account: "trabajo".into(),
                name: "Facturas".into(),
                conditions: RuleConditionsDraft {
                    from_addresses: vec!["alguien@example.com".into()],
                    sender_contains: Vec::new(),
                    subject_contains: vec!["factura".into()],
                },
                actions: RuleActionsDraft {
                    move_to_folder: Some(RuleFolder::JunkEmail),
                    mark_as_read: true,
                    assign_categories: vec!["facturas".into()],
                },
            }))
        );
    }

    #[test]
    fn a_custom_folder_is_an_id_and_the_two_safe_folders_are_named() {
        let mut arguments = create_args();
        arguments["actions"]["move_to_folder"] = json!("AAMkAD-carpeta");
        let parsed = super::parse(&request("mail.rule_create", arguments)).expect("parsed");
        let MailCall::RuleCreate(rule) = parsed else {
            panic!("not a rule");
        };
        assert_eq!(
            rule.actions.move_to_folder,
            Some(RuleFolder::Folder("AAMkAD-carpeta".into()))
        );
    }

    #[test]
    fn forwarding_and_unknown_fields_are_refused() {
        for arguments in [
            json!({
                "account": "trabajo",
                "name": "Reenvío",
                "conditions": {"subject_contains": ["factura"]},
                "actions": {"forwardTo": ["tercero@example.com"]}
            }),
            json!({
                "account": "trabajo",
                "name": "Reenvío",
                "forwardTo": "tercero@example.com",
                "conditions": {"subject_contains": ["factura"]},
                "actions": {"mark_as_read": true}
            }),
            json!({
                "account": "trabajo",
                "name": "Borrado",
                "conditions": {"subject_contains": ["factura"]},
                "actions": {"delete": true}
            }),
        ] {
            assert_eq!(
                refused("mail.rule_create", arguments).code,
                "mail_invalid_input"
            );
        }
    }

    #[test]
    fn the_lists_keep_their_limits_and_one_line() {
        let many: Vec<String> = (0..11).map(|n| format!("dominio{n}.example")).collect();
        let mut arguments = create_args();
        arguments["conditions"]["sender_contains"] = json!(many);
        assert_eq!(
            refused("mail.rule_create", arguments).field,
            Some("sender_contains")
        );

        let mut arguments = create_args();
        arguments["name"] = json!("dos\nlíneas");
        assert_eq!(refused("mail.rule_create", arguments).field, Some("name"));

        let many: Vec<String> = (0..6).map(|n| format!("categoría{n}")).collect();
        let mut arguments = create_args();
        arguments["actions"]["assign_categories"] = json!(many);
        assert_eq!(
            refused("mail.rule_create", arguments).field,
            Some("assign_categories")
        );
    }
}
