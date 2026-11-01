mod support;

use hn_core_mail::{Account, AccountStore, FileKey, MailError, Secret, Tenant, Tokens};
use hn_core_test_dir::TestDir;

const ACCESS: &str = "access-token-en-claro-que-no-debe-tocar-el-disco";
const REFRESH: &str = "refresh-token-en-claro-que-no-debe-tocar-el-disco";

fn account(label: &str) -> Account {
    Account::new(
        label,
        Tenant::Organizations,
        "ana@example.com",
        Tokens::new(ACCESS, Some(Secret::new(REFRESH)), 3600),
    )
}

#[test]
fn an_account_round_trips_and_never_lands_in_clear_on_disk() {
    let dir = TestDir::new("mail-store");
    let accounts = dir.join("accounts");
    let key_path = dir.join("master.key");
    let store = AccountStore::open(&accounts, &FileKey::new(&key_path)).expect("store");
    store.save(&account("trabajo")).expect("save");

    let raw = std::fs::read(accounts.join("trabajo.json")).expect("account file");
    let text = String::from_utf8_lossy(&raw);
    assert!(
        !text.contains(ACCESS),
        "the access token is in clear: {text}"
    );
    assert!(
        !text.contains(REFRESH),
        "the refresh token is in clear: {text}"
    );
    assert!(text.contains("\"nonce\""));
    assert!(text.contains("\"data\""));
    assert!(key_path.exists(), "the master key file must exist");

    let reopened = AccountStore::open(&accounts, &FileKey::new(&key_path)).expect("reopen");
    let loaded = reopened.load("trabajo").expect("load").expect("account");
    assert_eq!(loaded.label, "trabajo");
    assert_eq!(loaded.tenant, Tenant::Organizations);
    assert_eq!(loaded.upn, "ana@example.com");
    assert_eq!(loaded.tokens.access_token(), ACCESS);
    assert_eq!(loaded.tokens.refresh_token(), Some(REFRESH));
    assert_eq!(loaded.tokens.expires_in(), 3600);
}

#[test]
fn labels_list_the_accounts_and_remove_forgets_one() {
    let dir = TestDir::new("mail-store-labels");
    let store =
        AccountStore::open(dir.join("accounts"), &FileKey::new(dir.join("key"))).expect("store");
    store.save(&account("trabajo")).expect("save");
    store.save(&account("personal")).expect("save");
    assert_eq!(store.labels().expect("labels"), ["personal", "trabajo"]);

    store.remove("trabajo").expect("remove");
    store.remove("trabajo").expect("removing twice is fine");

    assert_eq!(store.labels().expect("labels"), ["personal"]);
    assert!(store.load("trabajo").expect("load").is_none());
}

#[test]
fn a_missing_account_is_none() {
    let dir = TestDir::new("mail-store-missing");
    let store =
        AccountStore::open(dir.join("accounts"), &FileKey::new(dir.join("key"))).expect("store");
    assert!(store.load("nadie").expect("load").is_none());
}

#[test]
fn a_tampered_account_file_is_refused() {
    let dir = TestDir::new("mail-store-tampered");
    let accounts = dir.join("accounts");
    let store = AccountStore::open(&accounts, &FileKey::new(dir.join("key"))).expect("store");
    store.save(&account("trabajo")).expect("save");

    let path = accounts.join("trabajo.json");
    let text = std::fs::read_to_string(&path).expect("read");
    let tampered = text.replacen("\"data\":\"", "\"data\":\"AAAA", 1);
    std::fs::write(&path, tampered).expect("write");
    let error = store.load("trabajo").expect_err("tampered");
    assert!(matches!(error, MailError::Store(_)));
    assert_eq!(error.code(), "mail_store_error");
}

#[test]
fn a_label_cannot_escape_the_accounts_directory() {
    let dir = TestDir::new("mail-store-label");
    let accounts = dir.join("accounts");
    let store = AccountStore::open(&accounts, &FileKey::new(dir.join("key"))).expect("store");
    let error = store.save(&account("../fuera")).expect_err("bad label");
    assert!(matches!(error, MailError::InvalidInput(_)));
    assert_eq!(error.code(), "mail_invalid_input");
    assert!(!dir.join("fuera.json").exists());
}
