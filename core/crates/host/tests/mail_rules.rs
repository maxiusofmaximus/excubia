//! The mailbox rule tools (F3B) over a scripted Microsoft and the real chat store.
//! Listing reads every rule, a forwarding one included and marked. Creating asks the
//! operator once, and Microsoft is called only after APPROVE, with the closed JSON of
//! the rule. A denied rule makes no call at all, and nothing unknown reaches a question.

mod mail_move_support;
mod mail_support;

use std::sync::Arc;
use std::time::Duration;

use excubia_chat::{ApprovalFilter, ApprovalStatus, ChatStore, Decision, ToolRequest};
use excubia_mail::{AccountStore, FileKey};
use excubia_seat::{
    MailCall, MailRuleDraft, RuleActionsDraft, RuleConditionsDraft, RuleFolder, RuleListQuery,
};
use excubia_test_dir::TestDir;
use mail_move_support::{actor, code_of, host, host_with_rules, scripted, written, ACCOUNT};
use mail_support::{ok, token_answer, ACCESS};
use serde_json::{json, Value};

const RULES_PATH: &str = "/me/mailFolders/inbox/messageRules";

/// The rule the tests ask for: a sender, a subject, a move to junk, a read mark and a category.
fn facturas() -> MailRuleDraft {
    MailRuleDraft {
        account: ACCOUNT.to_owned(),
        name: "Facturas".to_owned(),
        conditions: RuleConditionsDraft {
            from_addresses: vec!["alguien@example.com".to_owned()],
            sender_contains: Vec::new(),
            subject_contains: vec!["factura".to_owned()],
        },
        actions: RuleActionsDraft {
            move_to_folder: Some(RuleFolder::JunkEmail),
            mark_as_read: true,
            assign_categories: vec!["facturas".to_owned()],
        },
    }
}

fn create_call(draft: MailRuleDraft) -> MailCall {
    MailCall::RuleCreate(draft)
}

fn created_rule() -> Value {
    json!({
        "id": "RULE-1",
        "displayName": "Facturas",
        "isEnabled": true,
        "conditions": {
            "fromAddresses": [{"emailAddress": {"address": "alguien@example.com"}}],
            "subjectContains": ["factura"]
        },
        "actions": {
            "moveToFolder": "junkemail",
            "markAsRead": true,
            "assignCategories": ["facturas"]
        }
    })
}

/// Waits until a rule question is open, decides it, and returns its id. The tool call
/// waits for the decision, so the test decides from beside it.
async fn decide_when_asked(chat: &ChatStore, decision: Decision) -> String {
    for _ in 0..400 {
        let open = chat
            .approvals(&ApprovalFilter {
                status: Some(ApprovalStatus::Pending),
                conversation_id: None,
                limit: 5,
            })
            .expect("list");
        if let Some(approval) = open.first() {
            chat.decide_approval(&approval.id, decision, "device-1", "device-1")
                .expect("decide");
            return approval.id.clone();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("no rule question was asked");
}

#[tokio::test]
async fn listing_reads_every_rule_and_marks_the_forwarding_one() {
    let dir = TestDir::new("mail-rules-list");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let rules = json!({"value": [
        {
            "id": "R1",
            "displayName": "Facturas",
            "isEnabled": true,
            "conditions": {"fromAddresses": [{"emailAddress": {"address": "alguien@example.com"}}]},
            "actions": {"moveToFolder": "junkemail"}
        },
        {
            "id": "R2",
            "displayName": "Copia",
            "isEnabled": true,
            "conditions": {"subjectContains": ["factura"]},
            "actions": {"forwardTo": [{"emailAddress": {"address": "tercero@example.com"}}]}
        }
    ]});
    let transport = scripted(vec![token_answer(), ok(rules)]);
    let tools = host_with_rules(&dir, &transport, &chat);

    let reply = tools
        .call(
            &actor(),
            MailCall::RuleList(RuleListQuery {
                account: ACCOUNT.to_owned(),
            }),
        )
        .await
        .expect("list");

    let listed = &reply.result["rules"];
    assert_eq!(reply.result["untrusted_content"], true);
    assert_eq!(listed.as_array().map(Vec::len), Some(2));
    assert_eq!(listed[0]["forwards"], false);
    assert_eq!(listed[0]["actions"]["move_to_folder"], "junkemail");
    assert_eq!(listed[1]["name"], "Copia");
    assert_eq!(listed[1]["forwards"], true, "a forwarding rule is marked");
    assert_eq!(listed[1]["actions"]["other"], json!(["forwardTo"]));
    assert!(
        transport.asked()[1].ends_with(RULES_PATH),
        "{:?}",
        transport.asked()
    );
    assert!(
        transport.posted().is_empty(),
        "listing creates and changes nothing"
    );
    let audit = written(&dir.join("data"));
    assert!(audit.contains("mail.rule_list"), "the read is audited");
    assert!(
        saved_rules_access(&dir) == ACCESS,
        "the rotated pair is saved"
    );
}

#[tokio::test]
async fn an_approved_rule_is_created_with_the_closed_json_after_the_decision() {
    let dir = TestDir::new("mail-rules-approved");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(vec![token_answer(), ok(created_rule())]);
    let tools = host_with_rules(&dir, &transport, &chat);

    let seat = actor();
    let (reply, decided) = tokio::join!(
        tools.call(&seat, create_call(facturas())),
        decide_when_asked(&chat, Decision::Approve)
    );

    let reply = reply.expect("the rule is created");
    assert_eq!(reply.result["status"], "created");
    assert_eq!(reply.result["rule"]["id"], "RULE-1");
    assert_eq!(
        chat.approval(&decided).expect("approval").status,
        ApprovalStatus::Consumed,
        "the approval is spent by the creation"
    );
    let posts = transport.posted();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert!(posts[0].ends_with(RULES_PATH), "{posts:?}");
    assert_eq!(
        transport.posted_bodies()[0],
        json!({
            "displayName": "Facturas",
            "isEnabled": true,
            "conditions": {
                "fromAddresses": [{"emailAddress": {"address": "alguien@example.com"}}],
                "subjectContains": ["factura"]
            },
            "actions": {
                "moveToFolder": "junkemail",
                "markAsRead": true,
                "assignCategories": ["facturas"]
            }
        })
    );
}

#[tokio::test]
async fn the_card_names_the_rule_its_conditions_and_its_actions() {
    let dir = TestDir::new("mail-rules-card");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(vec![token_answer(), ok(created_rule())]);
    let tools = host_with_rules(&dir, &transport, &chat);

    let seat = actor();
    let (_, decided) = tokio::join!(
        tools.call(&seat, create_call(facturas())),
        decide_when_asked(&chat, Decision::Approve)
    );

    let approval = chat.approval(&decided).expect("approval");
    assert_eq!(approval.tool, "mail.rule");
    assert_eq!(approval.level.as_deref(), Some("user"));
    for part in [
        "«Facturas»",
        "desde alguien@example.com",
        "asunto contiene «factura»",
        "mover a no deseado",
        "marcar como leído",
        "categorías facturas",
    ] {
        assert!(
            approval.summary.contains(part),
            "{part} in {}",
            approval.summary
        );
    }
}

#[tokio::test]
async fn a_denied_rule_makes_no_call_and_is_not_asked_again_unchanged() {
    let dir = TestDir::new("mail-rules-denied");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host_with_rules(&dir, &transport, &chat);

    let seat = actor();
    let (refused, _) = tokio::join!(
        tools.call(&seat, create_call(facturas())),
        decide_when_asked(&chat, Decision::Deny)
    );

    assert_eq!(code_of(refused), "mail_approval_denied");
    assert!(transport.asked().is_empty(), "{:?}", transport.asked());

    let again = tools.call(&actor(), create_call(facturas())).await;
    assert_eq!(code_of(again), "mail_rule_refused");
    assert_eq!(chat.pending_approval_count().expect("count"), 0);
    assert!(transport.asked().is_empty(), "{:?}", transport.asked());
}

#[test]
fn an_argument_the_schema_does_not_name_never_becomes_a_call() {
    // The parser refuses it, so no MailCall exists to ask the operator about.
    let request = ToolRequest {
        tool: "mail.rule_create".to_owned(),
        arguments: json!({
            "account": ACCOUNT,
            "name": "Reenvío",
            "conditions": {"subject_contains": ["factura"]},
            "actions": {"forwardTo": ["tercero@example.com"]}
        }),
    };

    assert_eq!(
        code_of(excubia_seat::parse_tool(&request)),
        "mail_invalid_input"
    );
}

#[tokio::test]
async fn a_rule_the_host_cannot_build_is_refused_before_any_question() {
    let dir = TestDir::new("mail-rules-unbuildable");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host_with_rules(&dir, &transport, &chat);
    let mut draft = facturas();
    draft.conditions = RuleConditionsDraft::default();

    let refused = tools.call(&actor(), create_call(draft)).await;

    assert_eq!(code_of(refused), "mail_invalid_input");
    assert_eq!(chat.pending_approval_count().expect("count"), 0);
    assert!(transport.asked().is_empty());
}

#[tokio::test]
async fn without_rules_consent_nothing_is_asked_or_called() {
    let dir = TestDir::new("mail-rules-no-consent");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = scripted(Vec::new());
    let tools = host(&dir, &transport, &chat);

    let created = tools.call(&actor(), create_call(facturas())).await;
    let listed = tools
        .call(
            &actor(),
            MailCall::RuleList(RuleListQuery {
                account: ACCOUNT.to_owned(),
            }),
        )
        .await;

    assert_eq!(code_of(created), "mail_rules_not_enabled");
    assert_eq!(code_of(listed), "mail_rules_not_enabled");
    assert_eq!(chat.pending_approval_count().expect("count"), 0);
    assert!(transport.asked().is_empty(), "{:?}", transport.asked());
}

/// The access token of the rules consent as it is saved on disk.
fn saved_rules_access(dir: &TestDir) -> String {
    let key = FileKey::new(dir.join("key"));
    let store =
        AccountStore::open(dir.join("data").join("mail").join("accounts"), &key).expect("store");
    let account = store.load(ACCOUNT).expect("load").expect("account");
    account
        .rules
        .expect("rules consent")
        .access_token()
        .to_owned()
}
