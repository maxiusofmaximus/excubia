//! The owner's edit of a draft and the retry rules of the send, over HTTP
//! (MAILUX-HOST §B, §C, §D). The real host and the routed fake answer, so no test
//! reaches Microsoft.

mod harness;

use std::sync::Arc;

use excubia_auth::Permission;
use excubia_chat::{Decision, MailDraftCard, ManualClock, NewConversation};
use harness::mail_send_support::{ACCOUNT, BODY, DRAFT, TO};
use harness::TestHost;
use serde_json::{json, Value};

const ACCOUNTS: &str = "/v1/mail/accounts";
const SEND: &str = "/v1/mail/drafts/AAMk-draft-1/send";
const UPDATE: &str = "/v1/mail/drafts/AAMk-draft-1/update";
const REVIEW: &str = "/v1/mail/drafts/AAMk-draft-1?account=trabajo";
const REFUSED: &str = "el borrador fue rechazado; cámbialo para pedirlo de nuevo";
/// A Graph message id at its usual length, 140 characters.
const GRAPH_ID_CHARS: usize = 140;

/// An edit body: the account, and the fields the test changes.
fn edit_body(mut fields: Value) -> Option<Vec<u8>> {
    fields["account"] = json!(ACCOUNT);
    Some(fields.to_string().into_bytes())
}

/// A send body: the account, and the approval on the second call.
fn send_body(approval: Option<&str>) -> Option<Vec<u8>> {
    let mut body = json!({ "account": ACCOUNT });
    if let Some(approval) = approval {
        body["approval_id"] = json!(approval);
    }
    Some(body.to_string().into_bytes())
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

fn graph_sized_id() -> String {
    let id = format!("AAMkAGI2{}=", "x".repeat(GRAPH_ID_CHARS - 9));
    assert_eq!(id.chars().count(), GRAPH_ID_CHARS);
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_edits_a_draft_and_the_fields_left_out_stay() {
    let (host, _world) = TestHost::start_with_mail("mail-draft-edit").await;
    let owner = host
        .paired_with("dueno", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = owner
        .request(
            "POST",
            UPDATE,
            edit_body(json!({"subject": "Presupuesto de abril"})),
        )
        .await
        .expect("request");

    assert_eq!(status, 200, "{text}");
    let draft = &parse(&text)["draft"];
    assert_eq!(parse(&text)["ok"], json!(true));
    assert_eq!(draft["draft_id"], DRAFT);
    assert_eq!(draft["subject"], "Presupuesto de abril");
    assert_eq!(draft["to"], json!([TO]));
    assert_eq!(draft["body"], BODY);
    assert!(!draft["from"].as_str().unwrap_or_default().is_empty());

    let audit = std::fs::read_to_string(
        host.data_dir
            .join("mail")
            .join("accounts")
            .join("audit.jsonl"),
    )
    .expect("the audit");
    assert!(audit.contains("draft_edit"), "{audit}");
    assert!(!audit.contains(BODY), "the audit kept the body: {audit}");
    assert!(!audit.contains("Presupuesto de abril"), "{audit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owner_edit_is_sanitized_before_graph_and_the_review() {
    let (host, world) = TestHost::start_with_mail("mail-draft-edit-sanitize").await;
    let owner = host
        .paired_with("dueno", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = owner
        .request(
            "POST",
            UPDATE,
            edit_body(json!({
                "subject": "Presu\u{200B}puesto de abril",
                "body": "Hola\u{200B} Ana 👨\u{200D}👩\u{200D}👧"
            })),
        )
        .await
        .expect("request");

    assert_eq!(status, 200, "{text}");
    let clean_subject = "Presupuesto de abril";
    let clean = "Hola Ana 👨\u{200D}👩\u{200D}👧";
    assert_eq!(parse(&text)["draft"]["subject"], clean_subject);
    assert_eq!(parse(&text)["draft"]["body"], clean);
    assert_eq!(world.subject(), clean_subject);
    assert_eq!(world.body(), clean);

    let (status, text) = owner.request("GET", REVIEW, None).await.expect("request");
    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["draft"]["subject"], clean_subject);
    assert_eq!(parse(&text)["draft"]["body"], clean);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draft_the_host_did_not_create_is_not_found_even_with_a_graph_sized_id() {
    let (host, _world) = TestHost::start_with_mail("mail-draft-unknown").await;
    let owner = host
        .paired_with("dueno", &[Permission::Approve, Permission::ReadState])
        .await;
    let id = graph_sized_id();

    let (status, text) = owner
        .request(
            "POST",
            &format!("/v1/mail/drafts/{id}/update"),
            edit_body(json!({"subject": "Nuevo"})),
        )
        .await
        .expect("request");
    assert_eq!(status, 404, "{text}");
    assert_eq!(parse(&text)["detail"]["code"], "mail_unknown_draft");

    // The send answers the same case with its existing `400`; the id is what this
    // test pins, so it reaches the ownership check and not the id check.
    let (status, text) = owner
        .request(
            "POST",
            &format!("/v1/mail/drafts/{id}/send"),
            send_body(None),
        )
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert_eq!(parse(&text)["detail"]["code"], "mail_unknown_draft");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_past_the_tools_limits_is_refused_and_changes_nothing() {
    let (host, _world) = TestHost::start_with_mail("mail-draft-limits").await;
    let owner = host
        .paired_with("dueno", &[Permission::Approve, Permission::ReadState])
        .await;

    for fields in [
        json!({"subject": "a".repeat(999)}),
        json!({"body": "hola\u{7}"}),
    ] {
        let (status, text) = owner
            .request("POST", UPDATE, edit_body(fields))
            .await
            .expect("request");
        assert_eq!(status, 400, "{text}");
        assert_eq!(parse(&text)["detail"]["code"], "mail_invalid_input");
    }

    let (status, text) = owner.request("GET", REVIEW, None).await.expect("request");
    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["draft"]["body"], BODY);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draft_without_a_sender_is_reviewed_with_the_account_address() {
    let (host, world) = TestHost::start_with_mail("mail-draft-from").await;
    let owner = host
        .paired_with(
            "dueno",
            &[
                Permission::Admin,
                Permission::Approve,
                Permission::ReadState,
            ],
        )
        .await;
    let (_, text) = owner.request("GET", ACCOUNTS, None).await.expect("request");
    let upn = parse(&text)["accounts"][0]["upn"]
        .as_str()
        .expect("the account address")
        .to_owned();
    assert!(!upn.is_empty());
    world.without_sender();

    let (status, text) = owner.request("GET", REVIEW, None).await.expect("request");

    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["draft"]["from"], upn);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_question_says_so_until_the_draft_changes_and_an_expired_one_is_asked_again() {
    let (host, _world) = TestHost::start_with_mail("mail-draft-retry").await;
    let clock = Arc::new(ManualClock::at(1_000_000));
    host.chat.set_clock(clock.clone());
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let expired = parse(&text)["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_owned();
    clock.advance_secs(600);

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "the expired question is asked again: {text}");
    let refused = parse(&text)["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_owned();
    assert_ne!(refused, expired);
    host.chat
        .decide_approval(&refused, Decision::Deny, "device-1", "device-1")
        .expect("the device refuses");

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert_eq!(parse(&text)["detail"]["code"], "mail_draft_refused");
    assert_eq!(parse(&text)["detail"]["message"], REFUSED);

    let (status, text) = approver
        .request(
            "POST",
            UPDATE,
            edit_body(json!({"subject": "Presupuesto corregido"})),
        )
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "a changed draft is asked again: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_draft_card_reaches_only_the_devices_that_read_state() {
    let (host, _world) = TestHost::start_with_mail("mail-draft-card").await;
    let conversation = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_owned()),
            project_id: None,
        })
        .expect("conversation");
    host.chat
        .put_mail_draft_card(
            Some(&conversation.id),
            MailDraftCard {
                account: ACCOUNT,
                draft_id: DRAFT,
                to: &[TO.to_owned()],
                subject: "Presupuesto",
                preview: BODY,
            },
        )
        .expect("the card is shown");
    let path = format!("/v1/chat/conversations/{}/timeline", conversation.id);
    let reader = host.paired_with("lector", &[Permission::ReadState]).await;
    // `paired_with` always grants READ_STATE; a plain pair holds no grant at all.
    let (outsider, _) = host.pair("sin-lectura").await;

    let (status, text) = reader.request("GET", &path, None).await.expect("request");

    assert_eq!(status, 200, "{text}");
    let entries = parse(&text)["entries"].clone();
    let card = entries
        .as_array()
        .expect("the entries")
        .iter()
        .find(|entry| entry["kind"] == "mail_draft")
        .unwrap_or_else(|| panic!("no draft card: {text}"))
        .clone();
    assert_eq!(card["account"], ACCOUNT);
    assert_eq!(card["draft_id"], DRAFT);
    assert_eq!(card["to"], json!([TO]));
    assert_eq!(card["subject"], "Presupuesto");
    assert_eq!(card["preview"], BODY);

    let (status, text) = outsider.request("GET", &path, None).await.expect("request");
    assert_eq!(status, 403, "{text}");
    assert!(!text.contains(TO), "{text}");
}
