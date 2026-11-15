//! The rules primitives (F3A) against a mock Graph: the separate consent, the
//! closed payload of a create, the forwarding mark of a listed rule and every
//! validation refusal. No socket is ever opened.

mod support;

use excubia_mail::{
    Account, MailError, RuleActions, RuleConditions, RuleDestination, RuleSpec, RulesClient,
    Tenant, Tokens,
};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::ACCOUNT;

const RULES_URL: &str = "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messageRules";
const LOGIN_ACCESS: &str = "login-access-token";
const SEND_ACCESS: &str = "send-access-token";
const RULES_ACCESS: &str = "rules-access-token";
const RULES: &str = r#"{"value":[
    {"id":"r-1","displayName":"Facturas","isEnabled":true,
     "conditions":{"fromAddresses":[{"emailAddress":{"address":"alguien@example.com"}}],
                   "senderContains":["example.com"],"subjectContains":["factura"]},
     "actions":{"moveToFolder":"junkemail","markAsRead":true,"assignCategories":["facturas"]}},
    {"id":"r-2","displayName":"Reenvío","isEnabled":false,
     "conditions":{"subjectContains":["urgente"]},
     "actions":{"forwardTo":[{"emailAddress":{"address":"tercero@example.com"}}],
                "stopProcessingRules":true}},
    {"id":"r-3","displayName":"Borrado",
     "conditions":{"hasAttachments":true,"senderContains":["banco.example"]},
     "actions":{"delete":true,"moveToFolder":"deleteditems","assignCategories":[]}},
    {"id":"r-4","displayName":"Adjunto reenviado",
     "conditions":{"subjectContains":["adjunto"]},
     "actions":{"forwardAsAttachmentTo":[{"emailAddress":{"address":"tercero@example.com"}}]}}
]}"#;

fn account(rules: bool) -> Account {
    let account = Account::new(
        ACCOUNT,
        Tenant::Organizations,
        "alguien@example.com",
        Tokens::new(LOGIN_ACCESS, None, 3600),
    )
    .with_send(Tokens::new(SEND_ACCESS, None, 3600));
    if rules {
        account.with_rules(Tokens::new(RULES_ACCESS, None, 3600))
    } else {
        account
    }
}

fn conditions() -> RuleConditions {
    RuleConditions {
        sender_contains: vec!["example.com".to_owned()],
        ..RuleConditions::default()
    }
}

fn actions() -> RuleActions {
    RuleActions {
        mark_as_read: true,
        ..RuleActions::default()
    }
}

#[tokio::test]
async fn listing_rules_summarizes_them_and_marks_a_forward_without_hiding_it() {
    let transport = MockTransport::new(vec![MockResponse::ok(RULES)]);
    let account = account(true);
    let rules = RulesClient::new(&transport, &account)
        .expect("rules client")
        .list_rules()
        .await
        .expect("rules");

    assert_eq!(rules.len(), 4);
    assert_eq!(rules[0].id, "r-1");
    assert_eq!(rules[0].name, "Facturas");
    assert!(rules[0].enabled);
    assert_eq!(rules[0].conditions.from_addresses, ["alguien@example.com"]);
    assert_eq!(rules[0].conditions.sender_contains, ["example.com"]);
    assert_eq!(rules[0].conditions.subject_contains, ["factura"]);
    assert_eq!(
        rules[0].actions.move_to_folder,
        Some(RuleDestination::JunkEmail)
    );
    assert!(rules[0].actions.mark_as_read);
    assert_eq!(rules[0].actions.assign_categories, ["facturas"]);
    assert!(!rules[0].forwards);

    assert_eq!(rules[1].name, "Reenvío");
    assert!(!rules[1].enabled);
    assert!(rules[1].forwards, "a forward rule is listed, never hidden");
    assert!(rules[1].actions.other.contains(&"forwardTo".to_owned()));
    assert!(rules[1]
        .actions
        .other
        .contains(&"stopProcessingRules".to_owned()));

    assert!(!rules[2].forwards);
    assert_eq!(
        rules[2].actions.move_to_folder,
        Some(RuleDestination::DeletedItems)
    );
    assert!(rules[2].actions.other.contains(&"delete".to_owned()));
    assert!(rules[2]
        .conditions
        .other
        .contains(&"hasAttachments".to_owned()));

    assert!(rules[3].forwards);
    assert!(rules[3]
        .actions
        .other
        .contains(&"forwardAsAttachmentTo".to_owned()));

    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].url, RULES_URL);
    assert_eq!(calls[0].bearer.as_deref(), Some(RULES_ACCESS));
}

#[tokio::test]
async fn without_the_rules_consent_listing_fails_before_any_request() {
    let transport = MockTransport::new(vec![]);
    let account = account(false);

    let error = match RulesClient::new(&transport, &account) {
        Ok(_) => panic!("the rules client must refuse an account without consent"),
        Err(error) => error,
    };

    assert!(matches!(error, MailError::RulesNotEnabled));
    assert_eq!(error.code(), "mail_rules_not_enabled");
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn creating_a_rule_sends_exactly_the_closed_payload() {
    let created = r#"{"id":"r-9","displayName":"Facturas","isEnabled":true,
        "conditions":{"fromAddresses":[{"emailAddress":{"address":"alguien@example.com"}}],
        "senderContains":["example.com"],"subjectContains":["factura"]},
        "actions":{"moveToFolder":"junkemail","markAsRead":true,
        "assignCategories":["facturas","proveedor"]}}"#;
    let transport = MockTransport::new(vec![MockResponse::created(created)]);
    let account = account(true);
    let spec = RuleSpec::new(
        "Facturas",
        RuleConditions {
            from_addresses: vec!["alguien@example.com".to_owned()],
            sender_contains: vec!["example.com".to_owned()],
            subject_contains: vec!["factura".to_owned()],
        },
        RuleActions {
            move_to_folder: Some(RuleDestination::JunkEmail),
            mark_as_read: true,
            assign_categories: vec!["facturas".to_owned(), "proveedor".to_owned()],
        },
    )
    .expect("spec");

    let rule = RulesClient::new(&transport, &account)
        .expect("rules client")
        .create_rule(&spec)
        .await
        .expect("create");

    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].url, RULES_URL);
    assert_eq!(calls[0].bearer.as_deref(), Some(RULES_ACCESS));
    assert_eq!(
        calls[0].json_body(),
        json!({
            "displayName": "Facturas",
            "isEnabled": true,
            "conditions": {
                "fromAddresses": [{"emailAddress": {"address": "alguien@example.com"}}],
                "senderContains": ["example.com"],
                "subjectContains": ["factura"],
            },
            "actions": {
                "moveToFolder": "junkemail",
                "markAsRead": true,
                "assignCategories": ["facturas", "proveedor"],
            },
        })
    );
    let payload = calls[0].text();
    for forbidden in [
        "forwardTo",
        "redirectTo",
        "forwardAsAttachment",
        "permanentDelete",
        "stopProcessingRules",
    ] {
        assert!(!payload.contains(forbidden), "{forbidden} in {payload}");
    }
    assert!(!payload.contains("\"delete\""), "{payload}");
    assert_eq!(rule.id, "r-9");
    assert!(rule.enabled);
    assert_eq!(
        rule.actions.move_to_folder,
        Some(RuleDestination::JunkEmail)
    );
}

#[tokio::test]
async fn without_the_rules_consent_creating_fails_before_any_request() {
    let transport = MockTransport::new(vec![]);
    let account = account(false);
    let spec = RuleSpec::new("Regla", conditions(), actions()).expect("spec");

    let error = match RulesClient::new(&transport, &account) {
        Ok(_) => panic!("the rules client must refuse an account without consent"),
        Err(error) => error,
    };

    assert!(matches!(error, MailError::RulesNotEnabled));
    assert!(transport.calls().is_empty());
    assert_eq!(spec.name(), "Regla");
}

#[tokio::test]
async fn a_custom_folder_is_checked_against_the_mailbox_before_the_post() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(r#"{"id":"F-PROPIA"}"#),
        MockResponse::created(
            r#"{"id":"r-10","displayName":"Clientes","isEnabled":true,
            "conditions":{"senderContains":["example.com"]},
            "actions":{"moveToFolder":"F-PROPIA"}}"#,
        ),
    ]);
    let account = account(true);
    let spec = RuleSpec::new(
        "Clientes",
        conditions(),
        RuleActions {
            move_to_folder: Some(RuleDestination::Folder("F-PROPIA".to_owned())),
            ..RuleActions::default()
        },
    )
    .expect("spec");

    let rule = RulesClient::new(&transport, &account)
        .expect("rules client")
        .create_rule(&spec)
        .await
        .expect("create");

    let calls = transport.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(
        calls[0].url,
        "https://graph.microsoft.com/v1.0/me/mailFolders/F-PROPIA"
    );
    assert_eq!(calls[1].method, "POST");
    assert_eq!(calls[1].json_body()["actions"]["moveToFolder"], "F-PROPIA");
    assert_eq!(
        rule.actions.move_to_folder,
        Some(RuleDestination::Folder("F-PROPIA".to_owned()))
    );
}

#[tokio::test]
async fn a_folder_outside_the_mailbox_is_refused_before_the_post() {
    let transport = MockTransport::new(vec![MockResponse::status(
        404,
        r#"{"error":{"code":"ErrorItemNotFound","message":"The specified object was not found in the store."}}"#,
    )]);
    let account = account(true);
    let spec = RuleSpec::new(
        "Clientes",
        conditions(),
        RuleActions {
            move_to_folder: Some(RuleDestination::Folder("F-AJENA".to_owned())),
            ..RuleActions::default()
        },
    )
    .expect("spec");

    let error = RulesClient::new(&transport, &account)
        .expect("rules client")
        .create_rule(&spec)
        .await
        .expect_err("a foreign folder");

    assert_eq!(error.code(), "mail_invalid_input");
    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
}

#[test]
fn a_rule_needs_a_name_of_one_to_a_hundred_characters() {
    assert!(RuleSpec::new("R", conditions(), actions()).is_ok());
    assert!(RuleSpec::new("", conditions(), actions()).is_err());

    let too_long = "x".repeat(101);
    let error = RuleSpec::new(too_long.clone(), conditions(), actions()).expect_err("long name");
    assert_eq!(error.code(), "mail_invalid_input");
    assert!(!format!("{error}").contains(&too_long));
    assert!(RuleSpec::new("x".repeat(100), conditions(), actions()).is_ok());
}

#[test]
fn a_rule_needs_at_least_one_condition() {
    let error =
        RuleSpec::new("Regla", RuleConditions::default(), actions()).expect_err("no conditions");
    assert_eq!(error.code(), "mail_invalid_input");
    assert!(format!("{error}").contains("at least one condition"));
}

#[test]
fn a_condition_list_holds_at_most_ten_entries() {
    let eleven = vec!["example.com".to_owned(); 11];
    for list in 0..3 {
        let conditions = match list {
            0 => RuleConditions {
                from_addresses: eleven.clone(),
                ..RuleConditions::default()
            },
            1 => RuleConditions {
                sender_contains: eleven.clone(),
                ..RuleConditions::default()
            },
            _ => RuleConditions {
                subject_contains: eleven.clone(),
                ..RuleConditions::default()
            },
        };
        let error = RuleSpec::new("Regla", conditions, actions()).expect_err("eleven entries");
        assert!(format!("{error}").contains("at most 10"));
    }

    let ten = vec!["example.com".to_owned(); 10];
    let conditions = RuleConditions {
        from_addresses: ten.clone(),
        sender_contains: ten.clone(),
        subject_contains: ten,
    };
    assert!(RuleSpec::new("Regla", conditions, actions()).is_ok());
}

#[test]
fn a_condition_text_is_one_to_two_hundred_characters_without_line_breaks() {
    for text in [
        String::new(),
        "x".repeat(201),
        "linea\nsiguiente".to_owned(),
        "linea\rsiguiente".to_owned(),
    ] {
        let conditions = RuleConditions {
            sender_contains: vec![text],
            ..RuleConditions::default()
        };
        let error = RuleSpec::new("Regla", conditions, actions()).expect_err("bad text");
        assert_eq!(error.code(), "mail_invalid_input");
    }

    let conditions = RuleConditions {
        sender_contains: vec!["á".repeat(200)],
        ..RuleConditions::default()
    };
    assert!(RuleSpec::new("Regla", conditions, actions()).is_ok());
}

#[test]
fn a_rule_needs_at_least_one_action() {
    let error =
        RuleSpec::new("Regla", conditions(), RuleActions::default()).expect_err("no actions");
    assert_eq!(error.code(), "mail_invalid_input");
    assert!(format!("{error}").contains("at least one action"));
}

#[test]
fn a_rule_accepts_at_most_five_categories() {
    let six = vec!["spam".to_owned(); 6];
    let error = RuleSpec::new(
        "Regla",
        conditions(),
        RuleActions {
            assign_categories: six,
            ..RuleActions::default()
        },
    )
    .expect_err("six categories");
    assert!(format!("{error}").contains("at most 5"));

    assert!(RuleSpec::new(
        "Regla",
        conditions(),
        RuleActions {
            assign_categories: vec!["spam".to_owned(); 5],
            ..RuleActions::default()
        },
    )
    .is_ok());
}

#[test]
fn a_category_is_one_to_fifty_characters() {
    for category in [String::new(), "x".repeat(51)] {
        let error = RuleSpec::new(
            "Regla",
            conditions(),
            RuleActions {
                assign_categories: vec![category],
                ..RuleActions::default()
            },
        )
        .expect_err("bad category");
        assert!(format!("{error}").contains("1-50"));
    }

    assert!(RuleSpec::new(
        "Regla",
        conditions(),
        RuleActions {
            assign_categories: vec!["á".repeat(50)],
            ..RuleActions::default()
        },
    )
    .is_ok());
}

#[test]
fn a_custom_folder_id_cannot_be_empty_or_carry_line_breaks() {
    for id in [String::new(), "a\nb".to_owned(), "x".repeat(201)] {
        let error = RuleSpec::new(
            "Regla",
            conditions(),
            RuleActions {
                move_to_folder: Some(RuleDestination::Folder(id)),
                ..RuleActions::default()
            },
        )
        .expect_err("bad folder id");
        assert_eq!(error.code(), "mail_invalid_input");
    }
}
