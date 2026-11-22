//! The mail routes over the real host (MAIL1D §1): the refusals a client sees,
//! the answer of an empty host and the permission each route needs.

mod harness;

use std::time::{SystemTime, UNIX_EPOCH};

use excubia_auth::Permission;
use excubia_chat::Decision;
use excubia_host::mail_tools::LEDGER_FILE;
use excubia_mail::{Ledger, SendRow, SendState};
use harness::mail_send_support::{
    ACCOUNT, BODY, DRAFT, LOGIN_REFRESH, SEND_ACCESS, SEND_REFRESH, TO,
};
use harness::TestHost;
use serde_json::{json, Value};

const START: &str = "/v1/mail/login/start";
const STATUS: &str = "/v1/mail/login/status?account=trabajo";
const ACCOUNTS: &str = "/v1/mail/accounts";
const SEND: &str = "/v1/mail/drafts/AAMk-draft-1/send";
const REVIEW: &str = "/v1/mail/drafts/AAMk-draft-1?account=trabajo";
const ENABLE_SEND: &str = "/v1/mail/send/enable";
const DISABLE_SEND: &str = "/v1/mail/send/disable";

fn start_body(account: &str, tenant: &str) -> Option<Vec<u8>> {
    Some(
        json!({ "account": account, "tenant": tenant })
            .to_string()
            .into_bytes(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_without_a_registration_refuses_a_login_with_its_code() {
    let host = TestHost::start("mail-http-unconfigured").await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, body) = admin
        .request("POST", START, start_body("trabajo", "organizations"))
        .await
        .expect("request");

    assert_eq!(status, 400, "{body}");
    assert!(body.contains("mail_not_configured"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tenant_outside_the_list_is_refused_before_the_host_is_asked() {
    let host = TestHost::start("mail-http-tenant").await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, body) = admin
        .request("POST", START, start_body("trabajo", "nube"))
        .await
        .expect("request");

    assert_eq!(status, 422, "{body}");
    assert!(body.contains("mail_invalid_input"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mail_routes_need_admin() {
    let host = TestHost::start("mail-http-admin").await;
    let reader = host.paired_with("lector", &[Permission::ReadState]).await;

    let calls = [
        ("POST", START, start_body("trabajo", "organizations")),
        ("GET", STATUS, None),
        ("GET", ACCOUNTS, None),
    ];
    for (method, path, body) in calls {
        let (status, text) = reader.request(method, path, body).await.expect("request");
        assert_eq!(status, 403, "{method} {path}: {text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_host_lists_no_accounts_and_knows_no_login() {
    let host = TestHost::start("mail-http-empty").await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, body) = admin.request("GET", ACCOUNTS, None).await.expect("request");
    assert_eq!(status, 200, "{body}");
    let listing: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(listing, json!({ "ok": true, "accounts": [] }));

    let (status, body) = admin.request("GET", STATUS, None).await.expect("request");
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("mail_invalid_input"), "{body}");
}

/// The body of a send call: the account, and the approval on the second call.
fn send_body(approval: Option<&str>) -> Option<Vec<u8>> {
    let mut body = json!({ "account": ACCOUNT });
    if let Some(approval) = approval {
        body["approval_id"] = json!(approval);
    }
    Some(body.to_string().into_bytes())
}

fn account_body(account: &str) -> Option<Vec<u8>> {
    Some(json!({ "account": account }).to_string().into_bytes())
}

fn answer_id(text: &str) -> String {
    let value: Value = serde_json::from_str(text).expect("json");
    value["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_owned()
}

/// No refusal carries a token, the body of the draft or one of its addresses.
fn assert_no_secret_in(text: &str) {
    for secret in [LOGIN_REFRESH, SEND_ACCESS, SEND_REFRESH, BODY, TO] {
        assert!(!text.contains(secret), "the answer leaks {secret}: {text}");
    }
}

/// Records `count` sends of `ACCOUNT` in the last hour, so the budget is spent.
fn spend_budget(host: &TestHost, count: usize) {
    let ledger = Ledger::open(&host.data_dir.join(LEDGER_FILE)).expect("ledger");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let created_at = i64::try_from(now).expect("time");
    for index in 0..count {
        ledger
            .insert_send(&SendRow {
                send_id: format!("spent-send-{index}"),
                draft_id: DRAFT.to_owned(),
                account: ACCOUNT.to_owned(),
                state: SendState::Sent,
                digest: "spent".to_owned(),
                approval_id: format!("spent-approval-{index}"),
                domains: vec!["example.org".to_owned()],
                created_at,
                failure: None,
            })
            .expect("a spent send");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_send_routes_need_approve_and_the_consent_needs_admin() {
    let (host, world) = TestHost::start_with_mail("mail-http-matrix").await;
    let agent = host
        .paired_with("agente", &[Permission::Respond, Permission::ReadState])
        .await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    for client in [&agent, &admin] {
        let (status, text) = client
            .request("POST", SEND, send_body(None))
            .await
            .expect("request");
        assert_eq!(status, 403, "{text}");
        let (status, text) = client.request("GET", REVIEW, None).await.expect("request");
        assert_eq!(status, 403, "{text}");
    }
    for client in [&agent, &approver] {
        let (status, text) = client
            .request("POST", ENABLE_SEND, account_body(ACCOUNT))
            .await
            .expect("request");
        assert_eq!(status, 403, "{text}");
    }

    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_grant_stops_the_send_and_keeps_the_secrets_out() {
    let (host, world) = TestHost::start_with_mail("mail-http-revoke").await;
    let admin = host
        .paired_with(
            "admin",
            &[
                Permission::Admin,
                Permission::Approve,
                Permission::ReadState,
            ],
        )
        .await;

    let (status, text) = admin
        .request("POST", DISABLE_SEND, account_body(ACCOUNT))
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["send_enabled"], json!(false));

    let (status, text) = admin
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_send_not_enabled"), "{text}");
    assert_no_secret_in(&text);
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn enabling_the_send_without_a_registration_is_refused_with_its_code() {
    let (host, _world) = TestHost::start_with_mail("mail-http-enable").await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, text) = admin
        .request("POST", ENABLE_SEND, account_body(ACCOUNT))
        .await
        .expect("request");

    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_not_configured"), "{text}");
    assert_no_secret_in(&text);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_question_is_refused_with_its_code_and_nothing_is_sent() {
    let (host, world) = TestHost::start_with_mail("mail-http-denied").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let approval = answer_id(&text);

    host.chat
        .decide_approval(&approval, Decision::Deny, "device-1", "device-1")
        .expect("the device decides");
    let (status, text) = approver
        .request("POST", SEND, send_body(Some(&approval)))
        .await
        .expect("request");

    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_approval_denied"), "{text}");
    assert_no_secret_in(&text);
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_answered_after_the_draft_changed_is_stale() {
    let (host, world) = TestHost::start_with_mail("mail-http-stale").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    let (status, text) = approver
        .request("POST", SEND, send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 202, "{text}");
    let approval = answer_id(&text);

    world.set_body("Otro texto, distinto del que se aprobó.");
    host.chat
        .decide_approval(&approval, Decision::Approve, "device-1", "device-1")
        .expect("the device decides");
    let (status, text) = approver
        .request("POST", SEND, send_body(Some(&approval)))
        .await
        .expect("request");

    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_approval_stale"), "{text}");
    assert_no_secret_in(&text);
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spent_hourly_budget_answers_429_and_says_when_to_retry() {
    let (host, world) = TestHost::start_with_mail("mail-http-budget").await;
    spend_budget(&host, 10);
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, retry_after, text) = approver
        .request_with_retry_after("POST", SEND, send_body(None))
        .await
        .expect("request");

    assert_eq!(status, 429, "{text}");
    assert!(text.contains("mail_send_rate_limited"), "{text}");
    assert_no_secret_in(&text);
    let seconds: u64 = retry_after
        .expect("a Retry-After header")
        .parse()
        .expect("seconds");
    assert!(seconds > 0);
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_draft_or_account_is_refused_before_microsoft_is_asked() {
    let (host, world) = TestHost::start_with_mail("mail-http-unknown").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    let reads = world.reads().len();

    let (status, text) = approver
        .request("POST", "/v1/mail/drafts/nadie/send", send_body(None))
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_unknown_draft"), "{text}");

    let (status, text) = approver
        .request("GET", "/v1/mail/drafts/nadie?account=trabajo", None)
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_unknown_draft"), "{text}");

    let (status, text) = approver
        .request("POST", SEND, account_body("nadie"))
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_invalid_input"), "{text}");

    assert_eq!(world.reads().len(), reads);
}

#[tokio::test(flavor = "multi_thread")]
async fn reviewing_a_draft_shows_its_body_and_changes_nothing() {
    let (host, world) = TestHost::start_with_mail("mail-http-review").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = approver
        .request("GET", REVIEW, None)
        .await
        .expect("request");

    assert_eq!(status, 200, "{text}");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["draft"]["draft_id"], DRAFT);
    assert_eq!(answer["draft"]["body"], BODY);
    assert_eq!(answer["draft"]["to"], json!([TO]));
    assert!(world.sends().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_and_the_cancel_of_an_unknown_send_are_invalid_input() {
    let (host, _world) = TestHost::start_with_mail("mail-http-send-id").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;

    let (status, text) = approver
        .request("GET", "/v1/mail/sends/nadie", None)
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_invalid_input"), "{text}");

    let (status, text) = approver
        .request("POST", "/v1/mail/sends/nadie/cancel", None)
        .await
        .expect("request");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mail_invalid_input"), "{text}");
}
