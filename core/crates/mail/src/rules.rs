//! Listing and creating inbox rules (F3A) over the rules consent.
//!
//! A rule can copy every future message to a third party, so the creation type
//! is closed: it has no forwarding, redirection, deletion or stop-processing
//! field, and the JSON is built only from the fields it has. Reading sees every
//! rule the mailbox already has; a rule that forwards is marked, never hidden.
//!
//! Contract: `docs/contracts/mail.md` (F3A).

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::MailError;
use crate::graph::{check_status, encode_segment, GRAPH_BASE};
use crate::inbox::parse;
use crate::redact::Redacted;
use crate::store::Account;
use crate::transport::HttpTransport;

const RULES_PATH: &str = "/me/mailFolders/inbox/messageRules";
/// Written by others, never by this client: they are read only to mark them.
const FORWARD_ACTIONS: [&str; 3] = ["forwardTo", "redirectTo", "forwardAsAttachmentTo"];
const MAX_CONDITIONS_PER_LIST: usize = 10;
const MAX_CONDITION_CHARS: usize = 200;
const MAX_RULE_NAME_CHARS: usize = 100;
const MAX_CATEGORIES: usize = 5;
const MAX_CATEGORY_CHARS: usize = 50;
const MAX_FOLDER_ID_CHARS: usize = 200;

/// The rules primitives bound to one account and its rules consent. Without
/// that consent nothing is built, so no request can reach Graph.
pub struct RulesClient<'a> {
    transport: &'a dyn HttpTransport,
    access_token: &'a str,
}

impl<'a> RulesClient<'a> {
    pub fn new(transport: &'a dyn HttpTransport, account: &'a Account) -> Result<Self, MailError> {
        let rules = account.rules.as_ref().ok_or(MailError::RulesNotEnabled)?;
        Ok(Self {
            transport,
            access_token: rules.access_token(),
        })
    }

    /// The inbox rules as the mailbox has them, including rules made elsewhere.
    pub async fn list_rules(&self) -> Result<Vec<Rule>, MailError> {
        let url = format!("{GRAPH_BASE}{RULES_PATH}");
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawRules = parse(&response, "the rule list is incomplete")?;
        Ok(raw.value.into_iter().map(RawRule::into_rule).collect())
    }

    /// Creates one rule. A custom destination folder is checked against this
    /// mailbox first; the POST carries only the closed payload of [`RuleSpec`].
    pub async fn create_rule(&self, spec: &RuleSpec) -> Result<Rule, MailError> {
        if let Some(RuleDestination::Folder(id)) = &spec.actions.move_to_folder {
            self.assert_folder(id).await?;
        }
        let url = format!("{GRAPH_BASE}{RULES_PATH}");
        let payload = spec.payload();
        let response = self
            .transport
            .post_json(&url, self.access_token, &payload)
            .await?;
        check_status(&response)?;
        let raw: RawRule = parse(&response, "the created rule is incomplete")?;
        Ok(raw.into_rule())
    }

    async fn assert_folder(&self, id: &str) -> Result<(), MailError> {
        let url = format!("{GRAPH_BASE}/me/mailFolders/{}", encode_segment(id));
        let response = self.transport.get_json(&url, self.access_token).await?;
        if response.status == 404 {
            return Err(invalid("the destination folder is not in this mailbox"));
        }
        check_status(&response)
    }
}

/// One rule as the mailbox has it. `forwards` is true when the rule has a
/// forwarding action, whatever else it does; such a rule is still listed in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub conditions: ConditionSummary,
    pub actions: ActionSummary,
    pub forwards: bool,
}

/// The conditions of a listed rule. `other` names the conditions this product
/// does not model, so the summary never hides part of the rule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConditionSummary {
    pub from_addresses: Vec<String>,
    pub sender_contains: Vec<String>,
    pub subject_contains: Vec<String>,
    pub other: Vec<String>,
}

/// The actions of a listed rule. `other` names the actions this product does
/// not create, such as a forwarding or a permanent delete.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionSummary {
    pub move_to_folder: Option<RuleDestination>,
    pub mark_as_read: bool,
    pub assign_categories: Vec<String>,
    pub other: Vec<String>,
}

/// Where a rule may move a message: the two safe well-known folders, or a
/// folder of this mailbox by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleDestination {
    JunkEmail,
    DeletedItems,
    Folder(String),
}

impl RuleDestination {
    fn graph_value(&self) -> &str {
        match self {
            Self::JunkEmail => "junkemail",
            Self::DeletedItems => "deleteditems",
            Self::Folder(id) => id,
        }
    }

    fn from_graph_value(value: &str) -> Self {
        match value {
            "junkemail" => Self::JunkEmail,
            "deleteditems" => Self::DeletedItems,
            other => Self::Folder(other.to_owned()),
        }
    }
}

/// The conditions a rule may have. At least one list must carry an entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleConditions {
    pub from_addresses: Vec<String>,
    pub sender_contains: Vec<String>,
    pub subject_contains: Vec<String>,
}

/// The actions a rule may have. At least one must be set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleActions {
    pub move_to_folder: Option<RuleDestination>,
    pub mark_as_read: bool,
    pub assign_categories: Vec<String>,
}

/// A rule this product may create. Its fields are the whole vocabulary: no
/// forwarding, redirecting, deletion or rule-stopping action exists here. The
/// name, the conditions and the actions are checked when it is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpec {
    name: String,
    conditions: RuleConditions,
    actions: RuleActions,
}

impl RuleSpec {
    pub fn new(
        name: impl Into<String>,
        conditions: RuleConditions,
        actions: RuleActions,
    ) -> Result<Self, MailError> {
        let name = name.into();
        if !(1..=MAX_RULE_NAME_CHARS).contains(&name.chars().count()) {
            return Err(invalid("a rule name is 1-100 characters"));
        }
        validate_conditions(&conditions)?;
        validate_actions(&actions)?;
        Ok(Self {
            name,
            conditions,
            actions,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn conditions(&self) -> &RuleConditions {
        &self.conditions
    }

    pub fn actions(&self) -> &RuleActions {
        &self.actions
    }

    fn payload(&self) -> Value {
        let mut conditions = Map::new();
        if !self.conditions.from_addresses.is_empty() {
            let addresses: Vec<Value> = self
                .conditions
                .from_addresses
                .iter()
                .map(|address| json!({"emailAddress": {"address": address}}))
                .collect();
            conditions.insert("fromAddresses".to_owned(), Value::Array(addresses));
        }
        if !self.conditions.sender_contains.is_empty() {
            conditions.insert(
                "senderContains".to_owned(),
                json!(self.conditions.sender_contains),
            );
        }
        if !self.conditions.subject_contains.is_empty() {
            conditions.insert(
                "subjectContains".to_owned(),
                json!(self.conditions.subject_contains),
            );
        }

        let mut actions = Map::new();
        if let Some(destination) = &self.actions.move_to_folder {
            actions.insert("moveToFolder".to_owned(), json!(destination.graph_value()));
        }
        if self.actions.mark_as_read {
            actions.insert("markAsRead".to_owned(), json!(true));
        }
        if !self.actions.assign_categories.is_empty() {
            actions.insert(
                "assignCategories".to_owned(),
                json!(self.actions.assign_categories),
            );
        }

        json!({
            "displayName": self.name,
            "isEnabled": true,
            "conditions": Value::Object(conditions),
            "actions": Value::Object(actions),
        })
    }
}

fn validate_conditions(conditions: &RuleConditions) -> Result<(), MailError> {
    if conditions.from_addresses.is_empty()
        && conditions.sender_contains.is_empty()
        && conditions.subject_contains.is_empty()
    {
        return Err(invalid(
            "a rule needs at least one condition: fromAddresses, senderContains or subjectContains",
        ));
    }
    for list in [
        &conditions.from_addresses,
        &conditions.sender_contains,
        &conditions.subject_contains,
    ] {
        if list.len() > MAX_CONDITIONS_PER_LIST {
            return Err(invalid("a condition list holds at most 10 entries"));
        }
        for text in list {
            let count = text.chars().count();
            if !(1..=MAX_CONDITION_CHARS).contains(&count) || text.contains(['\n', '\r']) {
                return Err(invalid(
                    "a condition text is 1-200 characters without line breaks",
                ));
            }
        }
    }
    Ok(())
}

fn validate_actions(actions: &RuleActions) -> Result<(), MailError> {
    if actions.move_to_folder.is_none()
        && !actions.mark_as_read
        && actions.assign_categories.is_empty()
    {
        return Err(invalid("a rule needs at least one action"));
    }
    if let Some(RuleDestination::Folder(id)) = &actions.move_to_folder {
        let count = id.chars().count();
        if !(1..=MAX_FOLDER_ID_CHARS).contains(&count) || id.contains(['\n', '\r']) {
            return Err(invalid(
                "a destination folder id is 1-200 characters without line breaks",
            ));
        }
    }
    if actions.assign_categories.len() > MAX_CATEGORIES {
        return Err(invalid("a rule accepts at most 5 categories"));
    }
    for category in &actions.assign_categories {
        if !(1..=MAX_CATEGORY_CHARS).contains(&category.chars().count()) {
            return Err(invalid("a category is 1-50 characters"));
        }
    }
    Ok(())
}

fn invalid(message: &'static str) -> MailError {
    MailError::InvalidInput(Redacted::new(message))
}

#[derive(Deserialize)]
struct RawRules {
    #[serde(default)]
    value: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    conditions: Option<Map<String, Value>>,
    #[serde(default)]
    actions: Option<Map<String, Value>>,
}

impl RawRule {
    fn into_rule(self) -> Rule {
        let mut conditions = ConditionSummary::default();
        let mut other_conditions = Vec::new();
        for (name, value) in self.conditions.unwrap_or_default() {
            match name.as_str() {
                "fromAddresses" => conditions.from_addresses = addresses(&value),
                "senderContains" => conditions.sender_contains = strings(&value),
                "subjectContains" => conditions.subject_contains = strings(&value),
                _ if is_set(&value) => other_conditions.push(name),
                _ => {}
            }
        }
        other_conditions.sort();
        conditions.other = other_conditions;

        let mut actions = ActionSummary::default();
        let mut other_actions = Vec::new();
        let mut forwards = false;
        for (name, value) in self.actions.unwrap_or_default() {
            match name.as_str() {
                "moveToFolder" => {
                    actions.move_to_folder = value
                        .as_str()
                        .filter(|folder| !folder.is_empty())
                        .map(RuleDestination::from_graph_value);
                }
                "markAsRead" => actions.mark_as_read = value.as_bool().unwrap_or(false),
                "assignCategories" => actions.assign_categories = strings(&value),
                _ => {
                    if is_set(&value) {
                        forwards |= FORWARD_ACTIONS.contains(&name.as_str());
                        other_actions.push(name);
                    }
                }
            }
        }
        other_actions.sort();
        actions.other = other_actions;

        Rule {
            id: self.id,
            name: self.display_name,
            enabled: self.is_enabled,
            conditions,
            actions,
            forwards,
        }
    }
}

fn addresses(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.pointer("/emailAddress/address"))
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn is_set(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(set) => *set,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::Number(_) => true,
    }
}
