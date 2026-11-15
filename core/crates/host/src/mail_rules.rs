//! The mailbox rule tools (F3B): list the rules of one account, and create one
//! rule after the operator's device approves it. The create arguments are the
//! closed vocabulary of [`RuleSpec`], built and checked before the question, so
//! an unknown or forwarding field never reaches an approval. A denied or expired
//! question creates nothing: no request is made to Microsoft.

use excubia_chat::{ApprovalKind, ApprovalStatus, ChatError, NewApproval, SeatActor};
use excubia_mail::{
    Account, MailError, ReadRecord, Redacted, RuleActions, RuleConditions, RuleDestination,
    RuleSpec,
};
use excubia_seat::{MailRuleDraft, RuleFolder, RuleListQuery};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::mail_read;
use crate::mail_tools::MailTools;

/// The action the approval is filed under. Also the `tool` of the question.
pub const ACTION: &str = "mail.rule";

/// How long the operator has to decide one rule. The rule is not a batch, so it
/// keeps the ordinary ceiling.
const TTL_SECONDS: u32 = 300;

impl MailTools {
    /// Lists the rules the mailbox has, including one made elsewhere. Read only.
    pub(crate) async fn rule_list(&self, query: &RuleListQuery) -> Result<Value, MailError> {
        let account = self.rules_session(&query.account).await?;
        let client = self.rules_client(&account)?;
        let rules = client.list_rules().await?;
        self.audit
            .append(&ReadRecord::new(&query.account, "mail.rule_list", None))?;
        Ok(mail_read::rules_reply(&rules))
    }

    /// Asks the operator before creating one rule. Only APPROVE reaches Graph;
    /// DENY and expiry answer `mail_approval_denied` with no request made.
    pub(crate) async fn rule_create(
        &self,
        actor: &SeatActor,
        draft: &MailRuleDraft,
    ) -> Result<Value, MailError> {
        let spec = spec_of(draft)?;
        self.require_rules(&draft.account)?;
        let digest = rule_digest(&draft.account, &spec);
        let question = NewApproval {
            conversation_id: (!actor.conversation_id.is_empty())
                .then(|| actor.conversation_id.clone()),
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: ACTION.to_owned(),
            summary: summary(&draft.account, &spec),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: digest,
            payload_redacted: Some(payload(&draft.account, &spec)),
            ttl_seconds: TTL_SECONDS,
        };
        let approval = self
            .chat
            .request_mail_rule_approval(question)
            .await
            .map_err(chat_error)?;
        match approval.status {
            ApprovalStatus::Approved | ApprovalStatus::Consumed => {}
            ApprovalStatus::Denied | ApprovalStatus::Expired => {
                return Err(MailError::ApprovalDenied)
            }
            ApprovalStatus::Pending => {
                return Err(MailError::ApprovalUnavailable(Redacted::new(
                    "the rule question is still open",
                )))
            }
        }
        let asked = spec_from_approval(&approval, &draft.account)?;
        if rule_digest(&draft.account, &asked) != approval.action_digest {
            return Err(MailError::RuleStale);
        }
        let account = self.rules_session(&draft.account).await?;
        let client = self.rules_client(&account)?;
        let created = client.create_rule(&asked).await?;
        Ok(json!({
            "ok": true,
            "status": "created",
            "rule": mail_read::rule_reply(&created),
        }))
    }

    /// Refuses before any question when the account has no rules consent.
    fn require_rules(&self, label: &str) -> Result<(), MailError> {
        let account = self.load_account(label)?;
        if account.rules.is_none() {
            return Err(MailError::RulesNotEnabled);
        }
        Ok(())
    }

    fn load_account(&self, label: &str) -> Result<Account, MailError> {
        self.store.load(label)?.ok_or_else(|| {
            MailError::InvalidInput(Redacted::new("no account with that label is signed in"))
        })
    }
}

/// The closed rule the operator is asked about. The host builds the rule type
/// before the approval, so its checks are the ones that decide whether to ask.
fn spec_of(draft: &MailRuleDraft) -> Result<RuleSpec, MailError> {
    RuleSpec::new(
        draft.name.clone(),
        RuleConditions {
            from_addresses: draft.conditions.from_addresses.clone(),
            sender_contains: draft.conditions.sender_contains.clone(),
            subject_contains: draft.conditions.subject_contains.clone(),
        },
        RuleActions {
            move_to_folder: draft.actions.move_to_folder.as_ref().map(destination_of),
            mark_as_read: draft.actions.mark_as_read,
            assign_categories: draft.actions.assign_categories.clone(),
        },
    )
}

fn destination_of(folder: &RuleFolder) -> RuleDestination {
    match folder {
        RuleFolder::JunkEmail => RuleDestination::JunkEmail,
        RuleFolder::DeletedItems => RuleDestination::DeletedItems,
        RuleFolder::Folder(id) => RuleDestination::Folder(id.clone()),
    }
}

/// What the operator reads on the card: the name, every condition and every
/// action. A custom folder is named by its id; the host does not read the mailbox
/// before the decision.
fn summary(account: &str, spec: &RuleSpec) -> String {
    let mut conditions = Vec::new();
    for address in &spec.conditions().from_addresses {
        conditions.push(format!("desde {address}"));
    }
    for text in &spec.conditions().sender_contains {
        conditions.push(format!("remitente contiene «{text}»"));
    }
    for text in &spec.conditions().subject_contains {
        conditions.push(format!("asunto contiene «{text}»"));
    }
    let mut actions = Vec::new();
    if let Some(destination) = &spec.actions().move_to_folder {
        actions.push(format!("mover a {}", destination_label(destination)));
    }
    if spec.actions().mark_as_read {
        actions.push("marcar como leído".to_owned());
    }
    if !spec.actions().assign_categories.is_empty() {
        actions.push(format!(
            "categorías {}",
            spec.actions().assign_categories.join(", ")
        ));
    }
    format!(
        "Crear regla «{}» en {account}: {} → {}",
        spec.name(),
        conditions.join("; "),
        actions.join(", ")
    )
}

fn destination_label(destination: &RuleDestination) -> String {
    match destination {
        RuleDestination::JunkEmail => "no deseado".to_owned(),
        RuleDestination::DeletedItems => "eliminados".to_owned(),
        RuleDestination::Folder(id) => format!("la carpeta {id}"),
    }
}

/// The payload the operator's device reads. It is the whole rule, so the card
/// can be rebuilt from it after the decision.
fn payload(account: &str, spec: &RuleSpec) -> Value {
    json!({
        "account": account,
        "name": spec.name(),
        "conditions": {
            "from_addresses": spec.conditions().from_addresses,
            "sender_contains": spec.conditions().sender_contains,
            "subject_contains": spec.conditions().subject_contains,
        },
        "actions": {
            "move_to_folder": spec.actions().move_to_folder.as_ref().map(destination_value),
            "mark_as_read": spec.actions().mark_as_read,
            "assign_categories": spec.actions().assign_categories,
        },
    })
}

fn destination_value(destination: &RuleDestination) -> &str {
    match destination {
        RuleDestination::JunkEmail => "junkemail",
        RuleDestination::DeletedItems => "deleteditems",
        RuleDestination::Folder(id) => id,
    }
}

/// The rule the approval was asked with, rebuilt from its payload. Another
/// tool's approval, or one of another account, is not this rule.
fn spec_from_approval(
    approval: &excubia_chat::Approval,
    account: &str,
) -> Result<RuleSpec, MailError> {
    let payload = approval.payload_redacted.as_ref().ok_or_else(not_a_rule)?;
    if approval.tool != ACTION || payload["account"].as_str() != Some(account) {
        return Err(not_a_rule());
    }
    let conditions = &payload["conditions"];
    let actions = &payload["actions"];
    RuleSpec::new(
        text(payload, "name")?,
        RuleConditions {
            from_addresses: strings(conditions, "from_addresses")?,
            sender_contains: strings(conditions, "sender_contains")?,
            subject_contains: strings(conditions, "subject_contains")?,
        },
        RuleActions {
            move_to_folder: actions["move_to_folder"].as_str().map(destination_from),
            mark_as_read: actions["mark_as_read"] == true,
            assign_categories: strings(actions, "assign_categories")?,
        },
    )
    .map_err(|_| not_a_rule())
}

fn destination_from(value: &str) -> RuleDestination {
    match value {
        "junkemail" => RuleDestination::JunkEmail,
        "deleteditems" => RuleDestination::DeletedItems,
        other => RuleDestination::Folder(other.to_owned()),
    }
}

fn text(item: &Value, field: &str) -> Result<String, MailError> {
    item[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(not_a_rule)
}

fn strings(item: &Value, field: &str) -> Result<Vec<String>, MailError> {
    item[field]
        .as_array()
        .ok_or_else(not_a_rule)?
        .iter()
        .map(|value| value.as_str().map(str::to_owned).ok_or_else(not_a_rule))
        .collect()
}

/// The digest the approval binds: the account and the whole closed rule. Asking
/// the same rule while its question is open returns the same approval.
fn rule_digest(account: &str, spec: &RuleSpec) -> String {
    let canonical = payload(account, spec);
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(canonical.to_string()))
    )
}

/// What the chat refused, in rule terms. A refused rule must change to be asked
/// again; a digest another question holds is a stale rule.
fn chat_error(error: ChatError) -> MailError {
    match error {
        ChatError::ApprovalDigestReused => MailError::RuleStale,
        ChatError::ApprovalDigestRefused => MailError::RuleRefused,
        ChatError::ApprovalAlreadyDecided => MailError::RuleStale,
        ChatError::NoSuchApproval => MailError::InvalidInput(Redacted::new("unknown approval")),
        other => MailError::ApprovalUnavailable(Redacted::new(other.to_string())),
    }
}

fn not_a_rule() -> MailError {
    MailError::InvalidInput(Redacted::new(
        "the approval is not a mailbox rule for this account",
    ))
}
