//! The vault of `chat.db` at HTTP level and on disk (V2): the state, the init with its mouse samples, the refusals that
//! change nothing, the finish, the restart with the saved key, and the one open path of the host.

mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use excubia_auth::Permission;
use excubia_chat::{NewConversation, BACKUP_NAME};
use excubia_host::audit::AUDIT_FILE;
use excubia_host::vault::{self, MISSING_KEY, REJECTED_KEY};
use excubia_host::{paths, DEFAULT_HOOK_TTL};
use excubia_persist::vault::{EntropyPool, VaultKey, VaultKeyStore};
use harness::{FakeSecret, TestHost};
use serde_json::Value;

const PLAIN_HEADER: &[u8] = b"SQLite format 3\0";

/// `count` packed records of 16 bytes that the pool counts: each position differs from the one before.
fn samples(count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(count * 16);
    for i in 0..count {
        let x = (i as i32) * 3;
        let y = ((i as i32) * 7) % 500;
        let micros = (i as u64) * 1_000 + 17;
        bytes.extend_from_slice(&x.to_le_bytes());
        bytes.extend_from_slice(&y.to_le_bytes());
        bytes.extend_from_slice(&micros.to_le_bytes());
    }
    bytes
}

fn init_body(samples: &[u8]) -> Option<Vec<u8>> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(samples);
    Some(serde_json::to_vec(&serde_json::json!({ "samples": encoded })).expect("json"))
}

fn chat_file(data_dir: &Path) -> PathBuf {
    paths::chat_db_path(data_dir)
}

fn keys_of(data_dir: &Path) -> VaultKeyStore<FakeSecret> {
    VaultKeyStore::new(FakeSecret::for_dir(data_dir))
}

fn create_chat(host: &TestHost, title: &str) {
    host.chat
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("chat");
}

fn json(body: &str) -> Value {
    serde_json::from_str(body).expect("json")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_state_starts_unprotected_and_only_an_admin_reads_it() {
    let host = TestHost::start("vault-status").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("GET", "/v1/vault/status", None)
        .await
        .expect("status");
    assert_eq!(status, 200, "{body}");
    let value = json(&body);
    assert_eq!(value["ok"], true);
    assert_eq!(value["state"], "unprotected");
    assert_eq!(value["backup"], false);
    assert_eq!(value["samples_needed"], 2048);

    let reader = host.paired_with("Lector", &[]).await;
    let (status, _) = reader
        .request("GET", "/v1/vault/status", None)
        .await
        .expect("status");
    assert_eq!(status, 403, "a device without ADMIN reads nothing here");
}

#[tokio::test(flavor = "multi_thread")]
async fn init_with_mouse_samples_protects_the_file_and_keeps_the_chats() {
    let host = TestHost::start("vault-init").await;
    create_chat(&host, "Trabajo");
    let admin = host
        .paired_with("Admin", &[Permission::Admin, Permission::ReadState])
        .await;

    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");
    let value = json(&body);
    let phrase = value["phrase"].as_str().expect("phrase").to_string();
    assert_eq!(phrase.split_whitespace().count(), 24, "a 24-word phrase");

    assert!(
        !fs::read(chat_file(&host.data_dir))
            .expect("chat.db")
            .starts_with(PLAIN_HEADER),
        "the file is no longer plain"
    );
    let (status, body) = admin
        .request("GET", "/v1/vault/status", None)
        .await
        .expect("status");
    assert_eq!(status, 200, "{body}");
    assert_eq!(json(&body)["state"], "protected");
    assert_eq!(json(&body)["backup"], true);

    let (status, body) = admin
        .request("GET", "/v1/chat/conversations", None)
        .await
        .expect("list");
    assert_eq!(status, 200, "{body}");
    assert!(
        body.contains("Trabajo"),
        "the chat survives the move: {body}"
    );

    let saved = keys_of(&host.data_dir)
        .load()
        .expect("read")
        .expect("the key is saved");
    let from_phrase = VaultKey::from_phrase(&phrase).expect("the phrase reads back");
    assert_eq!(
        from_phrase.sqlcipher_pragma().as_str(),
        saved.sqlcipher_pragma().as_str(),
        "the phrase is the saved key"
    );

    let audit = fs::read_to_string(host.data_dir.join(AUDIT_FILE)).expect("audit");
    assert!(audit.contains("vault_initialised"), "{audit}");
    assert!(!audit.contains(&phrase), "the phrase is never audited");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_or_bad_sample_set_changes_nothing() {
    let host = TestHost::start("vault-short").await;
    create_chat(&host, "Trabajo");
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let before = fs::read(chat_file(&host.data_dir)).expect("chat.db");

    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(100)))
        .await
        .expect("init");
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("vault_samples_insufficient"), "{body}");

    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&[0u8; 17]))
        .await
        .expect("init");
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("vault_samples_invalid"), "{body}");

    assert!(
        keys_of(&host.data_dir).load().expect("read").is_none(),
        "no key was kept"
    );
    assert_eq!(
        fs::read(chat_file(&host.data_dir)).expect("chat.db"),
        before,
        "the file is untouched"
    );
    assert!(
        !paths::vault_backup_dir(&host.data_dir).exists(),
        "no backup was made"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_init_is_a_conflict_and_a_device_without_admin_is_refused() {
    let host = TestHost::start("vault-conflict").await;
    let reader = host.paired_with("Responde", &[Permission::Respond]).await;
    let (status, body) = reader
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 403, "{body}");
    assert!(keys_of(&host.data_dir).load().expect("read").is_none());

    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");

    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init again");
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("vault_already_protected"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_removes_the_backup_and_a_second_finish_is_a_conflict() {
    let host = TestHost::start("vault-finish").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");
    let backup = paths::vault_backup_dir(&host.data_dir).join(BACKUP_NAME);
    assert!(backup.is_file(), "the plain copy waits for the owner");

    let (status, body) = admin
        .request("POST", "/v1/vault/finish", None)
        .await
        .expect("finish");
    assert_eq!(status, 200, "{body}");
    assert!(!backup.exists(), "the plain copy is gone");
    assert!(!paths::vault_backup_dir(&host.data_dir).exists());

    let (status, body) = admin
        .request("POST", "/v1/vault/finish", None)
        .await
        .expect("finish again");
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("vault_no_backup"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_the_saved_key_opens_the_chats() {
    let host = TestHost::start("vault-restart").await;
    create_chat(&host, "Trabajo");
    let admin = host
        .paired_with("Admin", &[Permission::Admin, Permission::ReadState])
        .await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");

    let host = host.restart(DEFAULT_HOOK_TTL).await;
    let admin = host
        .paired_with("Admin", &[Permission::Admin, Permission::ReadState])
        .await;
    let (status, body) = admin
        .request("GET", "/v1/chat/conversations", None)
        .await
        .expect("list");
    assert_eq!(status, 200, "{body}");
    assert!(
        body.contains("Trabajo"),
        "the chat reads back after the restart: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_its_key_an_encrypted_chat_is_refused_and_not_replaced() {
    let host = TestHost::start("vault-missing").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");
    let data_dir = host.data_dir.clone();
    let before = fs::read(chat_file(&data_dir)).expect("chat.db");
    keys_of(&data_dir).delete().expect("forget the key");

    let error = vault::open_chat_store(&data_dir, &keys_of(&data_dir))
        .err()
        .expect("the host refuses to start");
    assert_eq!(format!("{error}"), MISSING_KEY);
    assert_eq!(
        fs::read(chat_file(&data_dir)).expect("chat.db"),
        before,
        "no empty database was made over it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_does_not_open_the_file_is_refused_and_the_file_stays() {
    let host = TestHost::start("vault-wrong-key").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");
    let data_dir = host.data_dir.clone();
    let wrong = VaultKey::generate(&EntropyPool::new()).expect("key");
    keys_of(&data_dir).save(&wrong).expect("save the wrong key");
    let before = fs::read(chat_file(&data_dir)).expect("chat.db");

    let error = vault::open_chat_store(&data_dir, &keys_of(&data_dir))
        .err()
        .expect("refused");
    assert_eq!(format!("{error}"), REJECTED_KEY);
    assert_eq!(fs::read(chat_file(&data_dir)).expect("chat.db"), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phrase_gives_the_key_back_and_a_wrong_phrase_keeps_nothing() {
    let host = TestHost::start("vault-recover").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (status, body) = admin
        .request("POST", "/v1/vault/init", init_body(&samples(2048)))
        .await
        .expect("init");
    assert_eq!(status, 200, "{body}");
    let phrase = json(&body)["phrase"].as_str().expect("phrase").to_string();
    let data_dir = host.data_dir.clone();
    let keys = keys_of(&data_dir);
    let saved = keys.load().expect("read").expect("key");
    keys.delete().expect("forget");

    let stranger = VaultKey::generate(&EntropyPool::new()).expect("key");
    let stranger_phrase = stranger.to_phrase().expect("phrase").to_string();
    assert!(vault::recover(&data_dir, &keys, &stranger_phrase).is_err());
    assert!(
        keys.load().expect("read").is_none(),
        "a phrase that does not open the file keeps nothing"
    );

    vault::recover(&data_dir, &keys, &phrase).expect("recover");
    let back = keys.load().expect("read").expect("key");
    assert_eq!(
        back.sqlcipher_pragma().as_str(),
        saved.sqlcipher_pragma().as_str()
    );
}
