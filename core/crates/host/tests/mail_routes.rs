//! The mail routes' host side over a scripted transport (MAIL1D §1): what the
//! API carries is the user code and the status, never a device code or a token,
//! and every refusal leaves with its stable code.

mod mail_support;

use std::sync::Arc;
use std::time::Duration;

use hn_core_api::{MailActions, MailLoginStatus};
use hn_core_chat::ChatStore;
use hn_core_host::mail_api::MailLogins;
use hn_core_mail::{AccountStore, FileKey, HttpTransport};
use hn_core_test_dir::TestDir;
use mail_support::{device_answer, ok, token_answer, Scripted, ACCESS, DEVICE, REFRESH, USER_CODE};
use serde_json::json;

fn service(dir: &TestDir, transport: &Arc<Scripted>, client_id: Option<&str>) -> MailLogins {
    let store =
        AccountStore::open(dir.join("accounts"), &FileKey::new(dir.join("key"))).expect("store");
    let transport: Arc<dyn HttpTransport> = transport.clone();
    MailLogins::new(
        transport,
        Arc::new(store),
        client_id.map(str::to_owned),
        Arc::new(ChatStore::in_memory().expect("chat")),
    )
}

/// Polls the status of the account's latest login until it has an answer.
async fn settled(logins: &MailLogins, account: &str) -> MailLoginStatus {
    for _ in 0..500 {
        let status = MailActions::login_status(logins, account).expect("a login for the account");
        if status != MailLoginStatus::Pending {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the login never settled");
}

#[tokio::test]
async fn a_start_answers_with_the_user_code_and_never_the_device_code() {
    let dir = TestDir::new("mail-routes-start");
    let transport = Arc::new(Scripted::new(vec![device_answer()]));
    let logins = service(&dir, &transport, Some("app"));

    let start = MailActions::login_start(&logins, "trabajo", "organizations", None)
        .await
        .expect("start");

    assert_eq!(start.user_code, USER_CODE);
    let wire = serde_json::to_string(&start).expect("json");
    assert!(!wire.contains(DEVICE), "{wire}");
    assert!(!wire.contains("device_code"), "{wire}");
    settled(&logins, "trabajo").await;
}

#[tokio::test]
async fn a_finished_login_is_listed_without_any_token() {
    let dir = TestDir::new("mail-routes-ok");
    let transport = Arc::new(Scripted::new(vec![
        device_answer(),
        token_answer(),
        ok(json!({"userPrincipalName": "ana@example.com"})),
    ]));
    let logins = service(&dir, &transport, Some("app"));
    MailActions::login_start(&logins, "trabajo", "organizations", None)
        .await
        .expect("start");

    let status = settled(&logins, "trabajo").await;

    assert_eq!(
        status,
        MailLoginStatus::Ok {
            label: "trabajo".to_owned(),
            upn: "ana@example.com".to_owned(),
        }
    );
    let accounts = MailActions::accounts(&logins).expect("accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].label, "trabajo");
    assert_eq!(accounts[0].tenant, "organizations");
    assert_eq!(accounts[0].upn, "ana@example.com");
    let wire = serde_json::to_string(&accounts).expect("json")
        + &serde_json::to_string(&status).expect("json");
    for secret in [ACCESS, REFRESH, DEVICE] {
        assert!(!wire.contains(secret), "{wire}");
    }
}

#[tokio::test]
async fn without_a_registration_every_login_is_refused_as_not_configured() {
    let dir = TestDir::new("mail-routes-unconfigured");
    let transport = Arc::new(Scripted::new(Vec::new()));
    let logins = service(&dir, &transport, None);

    let failure = MailActions::login_start(&logins, "trabajo", "consumers", None)
        .await
        .expect_err("refused");

    assert_eq!(failure.code, "mail_not_configured");
    assert!(
        transport.asked().is_empty(),
        "no request may leave without a registration"
    );
}

#[tokio::test]
async fn an_unknown_tenant_or_an_unstarted_account_is_refused_with_its_code() {
    let dir = TestDir::new("mail-routes-unknown");
    let transport = Arc::new(Scripted::new(Vec::new()));
    let logins = service(&dir, &transport, Some("app"));

    let tenant = MailActions::login_start(&logins, "trabajo", "nube", None)
        .await
        .expect_err("tenant");
    assert_eq!(tenant.code, "mail_invalid_input");
    let account = MailActions::login_status(&logins, "nadie").expect_err("account");
    assert_eq!(account.code, "mail_invalid_input");
    assert!(transport.asked().is_empty());
}
