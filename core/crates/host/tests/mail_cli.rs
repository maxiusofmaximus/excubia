//! `excubia-host mail` against a scripted transport: no network, no Microsoft account.
//! The output must name the user code and the account, never a token.

mod mail_support;

use excubia_host::mail_cli::{self, MailAction, MailDeps};
use excubia_mail::auth::Tenant;
use excubia_mail::{AccountStore, FileKey, MailError};
use excubia_test_dir::TestDir;
use mail_support::{device_answer, ok, token_answer, Scripted, ACCESS, DEVICE, REFRESH, USER_CODE};
use serde_json::json;

fn store(dir: &TestDir) -> AccountStore {
    AccountStore::open(dir.join("accounts"), &FileKey::new(dir.join("key"))).expect("store")
}

async fn act(
    action: MailAction,
    transport: &Scripted,
    store: &AccountStore,
    client_id: Option<&str>,
) -> Result<String, MailError> {
    let deps = MailDeps {
        transport,
        store,
        client_id,
    };
    let mut out = Vec::new();
    mail_cli::run(action, &deps, &mut out).await?;
    Ok(String::from_utf8(out).expect("utf-8 output"))
}

fn login(account: &str, upn: Option<&str>) -> MailAction {
    MailAction::Login {
        account: account.to_owned(),
        tenant: Tenant::Organizations,
        upn: upn.map(str::to_owned),
    }
}

fn words(list: &[&str]) -> impl Iterator<Item = String> {
    list.iter()
        .map(|word| (*word).to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[tokio::test]
async fn login_prints_the_code_and_saves_the_account_without_a_token_in_the_output() {
    let dir = TestDir::new("mail-cli-login");
    let store = store(&dir);
    let transport = Scripted::new(vec![device_answer(), token_answer()]);

    let out = act(
        login("trabajo", Some("ana@example.com")),
        &transport,
        &store,
        Some("app"),
    )
    .await
    .expect("login");

    assert!(out.contains(USER_CODE), "{out}");
    assert!(
        out.contains("Cuenta trabajo guardada (ana@example.com)"),
        "{out}"
    );
    assert!(
        !out.contains(ACCESS) && !out.contains(REFRESH) && !out.contains(DEVICE),
        "{out}"
    );
    assert_eq!(
        transport.asked().len(),
        2,
        "device code and token only, no /me"
    );
}

#[tokio::test]
async fn without_a_upn_the_sign_in_name_is_read_from_me() {
    let dir = TestDir::new("mail-cli-upn");
    let store = store(&dir);
    let transport = Scripted::new(vec![
        device_answer(),
        token_answer(),
        ok(json!({"userPrincipalName": "luis@example.com"})),
    ]);

    let out = act(login("personal", None), &transport, &store, Some("app"))
        .await
        .expect("login");

    assert!(out.contains("(luis@example.com)"), "{out}");
    assert!(transport.asked()[2].ends_with("/me?$select=userPrincipalName"));
}

#[tokio::test]
async fn login_without_a_client_id_is_refused_before_any_request() {
    let dir = TestDir::new("mail-cli-unconfigured");
    let store = store(&dir);
    let transport = Scripted::new(vec![]);

    let error = act(login("trabajo", None), &transport, &store, None)
        .await
        .expect_err("not configured");

    assert_eq!(error.code(), "mail_not_configured");
    assert!(transport.asked().is_empty());
}

#[tokio::test]
async fn accounts_lists_label_tenant_and_upn_and_never_a_token() {
    let dir = TestDir::new("mail-cli-accounts");
    let store = store(&dir);
    let transport = Scripted::new(vec![device_answer(), token_answer()]);
    act(
        login("trabajo", Some("ana@example.com")),
        &transport,
        &store,
        Some("app"),
    )
    .await
    .expect("login");

    let out = act(MailAction::Accounts, &transport, &store, None)
        .await
        .expect("accounts");

    assert_eq!(out.trim(), "trabajo\torganizations\tana@example.com");
    assert!(!out.contains(ACCESS) && !out.contains(REFRESH), "{out}");
}

#[tokio::test]
async fn logout_forgets_the_account_and_says_so_when_it_was_not_there() {
    let dir = TestDir::new("mail-cli-logout");
    let store = store(&dir);
    let transport = Scripted::new(vec![device_answer(), token_answer()]);
    act(
        login("trabajo", Some("ana@example.com")),
        &transport,
        &store,
        Some("app"),
    )
    .await
    .expect("login");
    let logout = || MailAction::Logout {
        account: "trabajo".to_owned(),
    };

    let gone = act(logout(), &transport, &store, None)
        .await
        .expect("logout");
    let again = act(logout(), &transport, &store, None)
        .await
        .expect("second logout");

    assert!(gone.contains("olvidada"), "{gone}");
    assert!(again.contains("No hay cuenta trabajo"), "{again}");
    let listed = act(MailAction::Accounts, &transport, &store, None)
        .await
        .expect("accounts");
    assert!(listed.is_empty());
}

#[test]
fn login_needs_an_account_and_a_known_tenant() {
    let parsed = mail_cli::parse(words(&[
        "login",
        "--account",
        "trabajo",
        "--tenant",
        "organizations",
        "--upn",
        "ana@example.com",
    ]))
    .expect("a complete login");
    assert!(matches!(
        parsed.action,
        MailAction::Login {
            tenant: Tenant::Organizations,
            ..
        }
    ));

    assert!(mail_cli::parse(words(&["login", "--account", "trabajo"])).is_err());
    assert!(mail_cli::parse(words(&[
        "login",
        "--account",
        "trabajo",
        "--tenant",
        "nube"
    ]))
    .is_err());
}

#[test]
fn logout_takes_the_label_and_unknown_words_are_refused() {
    let parsed =
        mail_cli::parse(words(&["logout", "trabajo", "--data-dir", "D:/datos"])).expect("logout");
    assert!(matches!(parsed.action, MailAction::Logout { ref account } if account == "trabajo"));
    assert!(parsed.data_dir.is_some());

    assert!(mail_cli::parse(words(&["sincronizar"])).is_err());
    assert!(mail_cli::parse(words(&["accounts", "--tokens"])).is_err());
}
