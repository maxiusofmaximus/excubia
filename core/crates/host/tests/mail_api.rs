//! The mail login service behind the API routes, over a scripted transport. The
//! answers may carry no device code, token or upload URL.

mod mail_support;

use std::sync::Arc;
use std::time::Duration;

use hn_core_chat::ChatStore;
use hn_core_host::mail_api::{LoginStatus, MailLogins};
use hn_core_mail::auth::Tenant;
use hn_core_mail::{AccountStore, FileKey, HttpResponse, HttpTransport, MailError};
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

/// Polls the status until the background login has an answer.
async fn settled(logins: &MailLogins, login_id: &str) -> LoginStatus {
    for _ in 0..500 {
        let status = logins.status(login_id).expect("a known login");
        if status != LoginStatus::Pending {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the login never settled");
}

#[tokio::test]
async fn the_start_answer_has_the_user_code_and_never_the_device_code() {
    let dir = TestDir::new("mail-api-start");
    let transport = Arc::new(Scripted::new(vec![device_answer()]));
    let logins = service(&dir, &transport, Some("app"));

    let start = logins
        .start("trabajo", Tenant::Organizations, None)
        .await
        .expect("start");

    assert_eq!(start.user_code, USER_CODE);
    assert_eq!(start.verification_uri, "https://microsoft.com/devicelogin");
    let wire = serde_json::to_string(&start).expect("json");
    assert!(!wire.contains(DEVICE), "{wire}");
    settled(&logins, &start.login_id).await;
}

#[tokio::test]
async fn a_finished_login_saves_the_account_and_lists_it_without_tokens() {
    let dir = TestDir::new("mail-api-ok");
    let transport = Arc::new(Scripted::new(vec![
        device_answer(),
        token_answer(),
        ok(json!({"userPrincipalName": "ana@example.com"})),
    ]));
    let logins = service(&dir, &transport, Some("app"));
    let start = logins
        .start("trabajo", Tenant::Organizations, None)
        .await
        .expect("start");

    let status = settled(&logins, &start.login_id).await;

    assert_eq!(
        status,
        LoginStatus::Ok {
            label: "trabajo".to_owned(),
            upn: "ana@example.com".to_owned(),
        }
    );
    let accounts = logins.accounts().expect("accounts");
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].upn, "ana@example.com");
    let wire = serde_json::to_string(&accounts).expect("json");
    assert!(!wire.contains(ACCESS) && !wire.contains(REFRESH), "{wire}");
}

#[tokio::test]
async fn a_login_the_user_refuses_ends_as_denied() {
    let dir = TestDir::new("mail-api-denied");
    let refused = HttpResponse {
        status: 400,
        body: json!({"error": "access_denied"}).to_string(),
    };
    let transport = Arc::new(Scripted::new(vec![device_answer(), refused]));
    let logins = service(&dir, &transport, Some("app"));
    let start = logins
        .start("trabajo", Tenant::Organizations, None)
        .await
        .expect("start");

    assert_eq!(settled(&logins, &start.login_id).await, LoginStatus::Denied);
    assert!(logins.accounts().expect("accounts").is_empty());
}

#[tokio::test]
async fn a_login_id_this_host_never_issued_is_invalid_input() {
    let dir = TestDir::new("mail-api-unknown");
    let transport = Arc::new(Scripted::new(vec![]));
    let logins = service(&dir, &transport, Some("app"));

    let error = logins.status("no-existe").expect_err("unknown");

    assert!(matches!(error, MailError::InvalidInput(_)));
    assert_eq!(error.code(), "mail_invalid_input");
}

#[tokio::test]
async fn without_a_client_id_no_login_starts() {
    let dir = TestDir::new("mail-api-unconfigured");
    let transport = Arc::new(Scripted::new(vec![]));
    let logins = service(&dir, &transport, None);

    let error = logins
        .start("trabajo", Tenant::Organizations, None)
        .await
        .expect_err("not configured");

    assert_eq!(error.code(), "mail_not_configured");
    assert!(transport.asked().is_empty());
}
