//! The send flow over HTTP (MAIL2A3 §1, §2, §4): the accounts say whether sending
//! is on, the approval carries `details` only for the device that can approve,
//! and a send runs from the request to a cancel inside its window. Over the real
//! host and the routed fake, so no test reaches Microsoft.

mod harness;

use harness::mail_send_support::{ACCOUNT, BODY, LOGIN_REFRESH, SEND_ACCESS, SEND_REFRESH, TO};
use harness::TestHost;
use excubia_core_auth::Permission;
use excubia_core_chat::Decision;
use serde_json::{json, Value};

const ACCOUNTS: &str = "/v1/mail/accounts";
const SEND: &str = "/v1/mail/drafts/AAMk-draft-1/send";
const APPROVALS: &str = "/v1/chat/approvals";
const SUBJECT: &str = "Presupuesto";

/// The body of a send call: the account, and the approval on the second call.
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

/// The approval `id` as the inbox lists it.
fn listed(text: &str, id: &str) -> Value {
    parse(text)["approvals"]
        .as_array()
        .expect("the approvals")
        .iter()
        .find(|item| item["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("the approval {id} is not listed: {text}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_accounts_say_whether_sending_is_enabled_and_show_no_token() {
    let (host, _world) = TestHost::start_with_mail("mail-http-enabled").await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, text) = admin.request("GET", ACCOUNTS, None).await.expect("request");

    assert_eq!(status, 200, "{text}");
    let listing = parse(&text);
    assert_eq!(listing["accounts"][0]["label"], ACCOUNT);
    assert_eq!(listing["accounts"][0]["send_enabled"], json!(true));
    for secret in [LOGIN_REFRESH, SEND_ACCESS, SEND_REFRESH] {
        assert!(!text.contains(secret), "the listing leaks {secret}: {text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mail_details_reach_the_device_that_approves_and_no_one_else() {
    let (host, _world) = TestHost::start_with_mail("mail-http-details").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    let reader = host.paired_with("lector", &[Permission::ReadState]).await;
    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let approval = parse(&text)["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_owned();

    let (status, text) = approver
        .request("GET", APPROVALS, None)
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    let seen = listed(&text, &approval);
    let details = &seen["details"];
    assert_eq!(details["account"], ACCOUNT);
    assert_eq!(details["subject"], SUBJECT);
    assert_eq!(details["cc"], json!([]));
    assert!(details.get("from").is_some(), "{details}");
    assert!(details["to"]
        .as_array()
        .expect("the recipients")
        .iter()
        .any(|address| address == TO));
    assert_eq!(details["body_preview"], BODY);
    assert!(details["attachments"].is_array(), "{details}");
    assert!(seen.get("payload_redacted").is_none(), "{seen}");

    let (status, text) = reader
        .request("GET", APPROVALS, None)
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    let hidden = listed(&text, &approval);
    assert!(hidden.get("details").is_none(), "{hidden}");
    assert!(hidden.get("payload_redacted").is_none(), "{hidden}");
    assert!(!text.contains(TO), "a reader saw a recipient: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_runs_from_the_request_to_a_cancel_inside_its_window() {
    let (host, world) = TestHost::start_with_mail("mail-http-flow").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let asked = parse(&text);
    assert_eq!(asked["state"], "awaiting_approval");
    assert_eq!(asked["expires_in"], json!(120));
    let approval = asked["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_owned();

    host.chat
        .decide_approval(&approval, Decision::Approve, "device-1", "device-1")
        .expect("the device decides");
    let (status, text) = approver
        .request("POST", SEND, send_body(Some(&approval)))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let scheduled = parse(&text);
    assert_eq!(scheduled["state"], "pending");
    assert_eq!(scheduled["window_seconds"], json!(10));
    let send = scheduled["send_id"].as_str().expect("a send id").to_owned();
    let path = format!("/v1/mail/sends/{send}");

    let (status, text) = approver.request("GET", &path, None).await.expect("request");
    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["state"], "pending");

    let (status, text) = approver
        .request("POST", &format!("{path}/cancel"), None)
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    assert_eq!(parse(&text)["state"], "cancelled");

    let (_, text) = approver.request("GET", &path, None).await.expect("request");
    assert_eq!(parse(&text)["state"], "cancelled");
    assert!(world.sends().is_empty(), "a cancelled send reached Graph");
    assert!(!text.contains(SEND_ACCESS), "the status leaks a token");
}
