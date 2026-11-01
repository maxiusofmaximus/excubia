use super::*;
use serde_json::{json, Value};

fn request(tool: &str, arguments: Value) -> ToolRequest {
    ToolRequest {
        tool: tool.to_string(),
        arguments,
    }
}

fn create(arguments: Value) -> Result<MailCall, ToolError> {
    parse(&request("mail.draft_create", arguments))
}

#[test]
fn a_draft_is_typed_with_its_recipients_and_format() {
    let call = create(json!({
        "account": "trabajo",
        "to": ["ana@example.com"],
        "cc": [],
        "subject": "Hola",
        "body": "<p>Hola</p>",
        "body_format": "html"
    }))
    .expect("parsed");

    assert_eq!(
        call,
        MailCall::DraftCreate(DraftFields {
            account: "trabajo".into(),
            to: vec!["ana@example.com".into()],
            cc: vec![],
            subject: "Hola".into(),
            body: "<p>Hola</p>".into(),
            format: BodyFormat::Html,
        })
    );
}

#[test]
fn agent_is_a_body_format_next_to_text_and_html() {
    let agent = create(json!({
        "account": "t", "to": ["a@b.c"], "subject": "s", "body": "b", "body_format": "agent"
    }));
    assert!(matches!(
        agent,
        Ok(MailCall::DraftCreate(DraftFields {
            format: BodyFormat::Agent,
            ..
        }))
    ));

    let omitted = create(json!({"account": "t", "to": ["a@b.c"], "subject": "", "body": ""}));
    assert!(matches!(
        omitted,
        Ok(MailCall::DraftCreate(DraftFields {
            format: BodyFormat::Text,
            ..
        }))
    ));
}

#[test]
fn a_draft_needs_a_recipient_and_a_known_format() {
    let no_recipient = create(json!({"account": "t", "to": [], "subject": "", "body": ""}));
    assert_eq!(no_recipient.unwrap_err().code, "mail_invalid_input");

    let format = create(json!({
        "account": "t", "to": ["a@b.c"], "subject": "", "body": "", "body_format": "rtf"
    }));
    assert_eq!(format.unwrap_err().field, Some("body_format"));
}

#[test]
fn unnamed_fields_are_refused_so_no_send_argument_can_pass() {
    let extra = create(json!({
        "account": "t", "to": ["a@b.c"], "subject": "", "body": "", "send": true
    }));
    assert_eq!(extra.unwrap_err().code, "mail_invalid_input");

    let no_args = parse(&request("mail.accounts", json!({"all": true})));
    assert_eq!(no_args.unwrap_err().code, "mail_invalid_input");
}

#[test]
fn an_inbox_read_has_safe_defaults_and_bounded_fields() {
    let defaults = parse(&request("mail.inbox_list", json!({"account": "t"})));
    assert_eq!(
        defaults,
        Ok(MailCall::InboxList(InboxQuery {
            account: "t".into(),
            folder: InboxFolder::Inbox,
            limit: 10,
            unread_only: false,
            from: None,
        }))
    );

    let filtered = parse(&request(
        "mail.inbox_list",
        json!({"account": "t", "folder": "junkemail", "limit": 20, "unread_only": true, "from": "example.com"}),
    ));
    assert_eq!(
        filtered,
        Ok(MailCall::InboxList(InboxQuery {
            account: "t".into(),
            folder: InboxFolder::JunkEmail,
            limit: 20,
            unread_only: true,
            from: Some("example.com".into()),
        }))
    );

    for arguments in [
        json!({"account": "t", "limit": 0}),
        json!({"account": "t", "limit": 21}),
        json!({"account": "t", "folder": "drafts"}),
        json!({"account": "t", "unread_only": "yes"}),
        json!({"account": "t", "from": ""}),
        json!({"account": "t", "mark_read": true}),
    ] {
        let refused = parse(&request("mail.inbox_list", arguments.clone()));
        assert_eq!(
            refused.unwrap_err().code,
            "mail_invalid_input",
            "{arguments}"
        );
    }
}

#[test]
fn a_triage_reads_the_inbox_or_junk_at_most_ten_and_takes_no_filter() {
    assert_eq!(
        parse(&request("mail.triage", json!({"account": "t"}))),
        Ok(MailCall::Triage(TriageQuery {
            account: "t".into(),
            folder: InboxFolder::Inbox,
            limit: 10,
            unread_only: false,
        }))
    );
    assert_eq!(
        parse(&request(
            "mail.triage",
            json!({"account": "t", "folder": "junkemail", "limit": 1, "unread_only": true}),
        )),
        Ok(MailCall::Triage(TriageQuery {
            account: "t".into(),
            folder: InboxFolder::JunkEmail,
            limit: 1,
            unread_only: true,
        }))
    );

    for arguments in [
        json!({"account": "t", "limit": 0}),
        json!({"account": "t", "limit": 11}),
        json!({"account": "t", "folder": "sentitems"}),
        json!({"account": "t", "folder": "drafts"}),
        json!({"account": "t", "from": "example.com"}),
    ] {
        let refused = parse(&request("mail.triage", arguments.clone()));
        assert_eq!(
            refused.unwrap_err().code,
            "mail_invalid_input",
            "{arguments}"
        );
    }
}

#[test]
fn a_message_read_takes_an_account_and_an_id_only() {
    assert_eq!(
        parse(&request(
            "mail.message_get",
            json!({"account": "t", "message_id": "AAMk-1"})
        )),
        Ok(MailCall::MessageGet(MessageRef {
            account: "t".into(),
            message_id: "AAMk-1".into(),
        }))
    );

    let missing = parse(&request("mail.message_get", json!({"account": "t"})));
    assert_eq!(missing.unwrap_err().code, "mail_invalid_input");

    let extra = parse(&request(
        "mail.message_get",
        json!({"account": "t", "message_id": "AAMk-1", "attachment": "a.pdf"}),
    ));
    assert_eq!(extra.unwrap_err().code, "mail_invalid_input");
}

#[test]
fn controls_in_the_subject_and_the_path_are_refused() {
    let subject = create(json!({
        "account": "t", "to": ["a@b.c"], "subject": "a\u{0}b", "body": ""
    }));
    assert_eq!(subject.unwrap_err().field, Some("subject"));

    let path = parse(&request(
        "mail.attach_file",
        json!({"account": "t", "draft_id": "d1", "path": "C:\\x\u{7}.pdf"}),
    ));
    assert_eq!(path.unwrap_err().field, Some("path"));
}

#[test]
fn an_update_takes_the_same_fields_and_refuses_an_extra_one() {
    let update = |extra: Option<(&str, Value)>| {
        let mut arguments = json!({
            "account": "t", "draft_id": "d1", "to": ["a@b.c"], "subject": "s", "body": "b"
        });
        if let Some((key, value)) = extra {
            arguments[key] = value;
        }
        parse(&request("mail.draft_update", arguments))
    };

    assert!(matches!(update(None), Ok(MailCall::DraftUpdate(_))));
    let refused = update(Some(("send", json!(true))));
    assert_eq!(refused.unwrap_err().code, "mail_invalid_input");
}

#[test]
fn a_body_keeps_its_line_breaks_but_no_other_control() {
    assert!(body("uno\ndos\r\n\tcuatro").is_ok());
    assert!(body("uno\u{1b}[31m").is_err());
}

#[test]
fn the_four_tools_that_take_an_account_parse() {
    let draft = request("mail.draft_get", json!({"account": "t", "draft_id": "d1"}));
    assert!(matches!(parse(&draft), Ok(MailCall::DraftGet(_))));

    let attach = request(
        "mail.attach_file",
        json!({"account": "t", "draft_id": "d1", "path": "C:\\docs\\a.pdf"}),
    );
    assert!(matches!(parse(&attach), Ok(MailCall::AttachFile(_))));
    assert_eq!(
        parse(&request("mail.accounts", json!({}))),
        Ok(MailCall::Accounts)
    );
}

#[test]
fn a_folder_listing_takes_the_account_and_nothing_else() {
    assert_eq!(
        parse(&request("mail.folders", json!({"account": "trabajo"}))),
        Ok(MailCall::Folders(FoldersQuery {
            account: "trabajo".into(),
        }))
    );
    for arguments in [
        json!({}),
        json!({"account": "t", "folder": "inbox"}),
        json!({"account": "t", "move_to": "junkemail"}),
        json!({"account": ""}),
    ] {
        let refused = parse(&request("mail.folders", arguments.clone()));
        assert_eq!(
            refused.unwrap_err().code,
            "mail_invalid_input",
            "{arguments}"
        );
    }
}
