use std::fs;

use excubia_persist::vault::EntropyPool;
use excubia_test_dir::TestDir;

use super::*;
use crate::schema::{open, open_with_key, SearchMode, SCHEMA_VERSION};
use crate::schema_approval::{APPROVAL_SCHEMA, HOOK_TOKEN_SCHEMA};
use crate::schema_base::BASE_SCHEMA;
use crate::schema_team::TEAM_SCHEMA;
use crate::{ChatStore, NewConversation};

/// Letters and digits only, so the FTS5 MATCH takes it as one plain word.
const MARKER: &str = "marcadorbovedaZ9q";

fn fresh_key() -> VaultKey {
    VaultKey::generate(&EntropyPool::new()).expect("key")
}

/// A plain `chat.db` as the host leaves it: one conversation, one message with the marker.
fn plain_chat(path: &Path) {
    let store = ChatStore::open(path).expect("store");
    let conversation = store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation");
    store
        .append_user_message(&conversation.id, "client-1", &format!("Hola, {MARKER}"), None)
        .expect("message");
}

fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("read")
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn wal(path: &Path) -> PathBuf {
    with_suffix(path, "-wal")
}

#[test]
fn the_state_comes_from_the_first_bytes() {
    let dir = TestDir::new("vault-state");
    let path = dir.join("chat.db");
    assert_eq!(db_state(&path).expect("missing"), DbState::Missing);
    fs::write(&path, b"").expect("empty");
    assert_eq!(db_state(&path).expect("empty"), DbState::Missing);
    fs::write(&path, b"SQLite format 3\0and more").expect("plain");
    assert_eq!(db_state(&path).expect("plain"), DbState::Plain);
    fs::write(&path, b"\x9f\x01 not a plain file").expect("other");
    assert_eq!(db_state(&path).expect("other"), DbState::Encrypted);
}

/// A version-28 database with data becomes SQLCipher, keeps its rows and its search,
/// and no longer shows its text in the file or in the WAL.
#[test]
fn a_version_28_chat_is_encrypted_and_still_reads_and_searches() {
    let dir = TestDir::new("vault-v28");
    let path = dir.join("chat.db");
    plain_chat(&path);
    assert!(contains(&bytes(&path), MARKER), "plain text before the move");
    let key = fresh_key();

    let migration =
        migrate_plain_to_encrypted(&path, &key, &dir.join(".excubia-backups")).expect("migrate");

    assert_eq!(db_state(&path).expect("state"), DbState::Encrypted);
    assert!(!contains(&bytes(&path), MARKER), "no plain text in chat.db");
    if wal(&path).exists() {
        assert!(!contains(&bytes(&wal(&path)), MARKER), "no plain text in the WAL");
    }
    assert!(migration.tables > 0 && migration.rows > 0);
    assert_eq!(db_state(&migration.backup).expect("backup"), DbState::Plain);
    assert!(contains(&bytes(&migration.backup), MARKER), "the backup is the plain copy");

    let (connection, mode) = open_with_key(&path, Some(&key)).expect("open with the key");
    assert_eq!(mode, SearchMode::Fts5);
    let body: String = connection
        .query_row(
            "SELECT body_md FROM message WHERE body_md LIKE ?1",
            [format!("%{MARKER}%")],
            |row| row.get(0),
        )
        .expect("message");
    assert_eq!(body, format!("Hola, {MARKER}"));
    let hits: i64 = connection
        .query_row(
            "SELECT count(*) FROM conversation_fts WHERE conversation_fts MATCH ?1",
            [MARKER],
            |row| row.get(0),
        )
        .expect("search");
    assert_eq!(hits, 1);
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
}

/// A version-4 database is migrated as it is and upgraded on the first keyed open.
#[test]
fn a_legacy_version_four_chat_is_encrypted_and_upgraded_on_the_next_open() {
    let dir = TestDir::new("vault-v4");
    let path = dir.join("chat.db");
    {
        let legacy = Connection::open(&path).expect("legacy");
        legacy.execute_batch(BASE_SCHEMA).expect("base");
        legacy.execute_batch(TEAM_SCHEMA).expect("team");
        legacy.execute_batch(APPROVAL_SCHEMA).expect("approval");
        legacy.execute_batch(HOOK_TOKEN_SCHEMA).expect("hook tokens");
        legacy
            .execute_batch(
                "INSERT INTO company (id, name, created_at)
                    VALUES ('co_1', 'Legado', '2026-10-22T09:00:00.000Z');",
            )
            .expect("company");
        legacy.pragma_update(None, "user_version", 4).expect("version four");
    }
    let key = fresh_key();

    migrate_plain_to_encrypted(&path, &key, &dir.join(".excubia-backups")).expect("migrate");
    let (connection, _) = open_with_key(&path, Some(&key)).expect("open");

    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    let name: String = connection
        .query_row("SELECT name FROM company WHERE id = 'co_1'", [], |row| row.get(0))
        .expect("company");
    assert_eq!(name, "Legado");
}

#[test]
fn a_wrong_key_is_refused_and_the_encrypted_file_is_not_touched() {
    let dir = TestDir::new("vault-wrong-key");
    let path = dir.join("chat.db");
    plain_chat(&path);
    let key = fresh_key();
    migrate_plain_to_encrypted(&path, &key, &dir.join(".excubia-backups")).expect("migrate");
    let before = bytes(&path);

    let wrong = fresh_key();
    assert!(matches!(
        open_with_key(&path, Some(&wrong)),
        Err(ChatError::VaultKeyRejected)
    ));
    assert_eq!(bytes(&path), before);
}

/// Without the key an encrypted chat is not opened, and nothing is written to it.
#[test]
fn an_encrypted_chat_opened_without_the_key_is_refused_and_untouched() {
    let dir = TestDir::new("vault-no-key");
    let path = dir.join("chat.db");
    plain_chat(&path);
    migrate_plain_to_encrypted(&path, &fresh_key(), &dir.join(".excubia-backups")).expect("migrate");
    let before = bytes(&path);

    assert!(open(&path).is_err());
    assert_eq!(bytes(&path), before);
}

/// A plain file is never replaced by a new empty keyed database.
#[test]
fn a_plain_file_is_refused_with_a_key_and_not_replaced() {
    let dir = TestDir::new("vault-plain-with-key");
    let path = dir.join("chat.db");
    plain_chat(&path);
    let before = bytes(&path);

    assert!(matches!(
        open_with_key(&path, Some(&fresh_key())),
        Err(ChatError::VaultKeyRejected)
    ));
    assert_eq!(bytes(&path), before);
}

#[test]
fn an_encrypted_chat_is_not_migrated_twice() {
    let dir = TestDir::new("vault-twice");
    let path = dir.join("chat.db");
    plain_chat(&path);
    let key = fresh_key();
    let backups = dir.join(".excubia-backups");
    migrate_plain_to_encrypted(&path, &key, &backups).expect("first");
    let before = bytes(&path);

    assert!(matches!(
        migrate_plain_to_encrypted(&path, &key, &backups),
        Err(ChatError::VaultAlreadyEncrypted)
    ));
    assert_eq!(bytes(&path), before);
}

#[test]
fn a_missing_chat_has_nothing_to_migrate() {
    let dir = TestDir::new("vault-missing");
    let path = dir.join("chat.db");
    assert!(matches!(
        migrate_plain_to_encrypted(&path, &fresh_key(), &dir.join(".excubia-backups")),
        Err(ChatError::VaultMissing)
    ));
    assert!(!path.exists());
}

#[test]
fn a_backup_folder_that_cannot_be_made_leaves_chat_db_as_it_was() {
    let dir = TestDir::new("vault-no-backup");
    let path = dir.join("chat.db");
    plain_chat(&path);
    let before = bytes(&path);
    let blocked = dir.join("blocked");
    fs::write(&blocked, b"a file, not a folder").expect("blocker");

    assert!(migrate_plain_to_encrypted(&path, &fresh_key(), &blocked).is_err());
    assert_eq!(bytes(&path), before);
    assert_eq!(db_state(&path).expect("state"), DbState::Plain);
    assert!(!with_suffix(&path, TMP_SUFFIX).exists());
}

#[test]
fn a_directory_where_the_copy_goes_stops_the_move_and_chat_db_stays() {
    let dir = TestDir::new("vault-tmp-in-the-way");
    let path = dir.join("chat.db");
    plain_chat(&path);
    let before = bytes(&path);
    fs::create_dir(with_suffix(&path, TMP_SUFFIX)).expect("directory");

    assert!(migrate_plain_to_encrypted(&path, &fresh_key(), &dir.join(".excubia-backups")).is_err());
    assert_eq!(bytes(&path), before);
}

/// An attempt that died halfway leaves its temporary file; the next attempt starts over.
#[test]
fn a_stale_copy_from_a_dead_attempt_is_discarded() {
    let dir = TestDir::new("vault-stale");
    let path = dir.join("chat.db");
    plain_chat(&path);
    fs::write(with_suffix(&path, TMP_SUFFIX), b"half written").expect("stale");
    let key = fresh_key();

    migrate_plain_to_encrypted(&path, &key, &dir.join(".excubia-backups")).expect("retry");

    assert_eq!(db_state(&path).expect("state"), DbState::Encrypted);
    assert!(!with_suffix(&path, TMP_SUFFIX).exists());
    assert!(open_with_key(&path, Some(&key)).is_ok());
}

/// The copy is accepted only when it matches the plain file exactly.
#[test]
fn the_copy_must_match_the_source_exactly() {
    let dir = TestDir::new("vault-verify");
    let path = dir.join("chat.db");
    let key = fresh_key();
    let (connection, _) = open_with_key(&path, Some(&key)).expect("keyed");
    let actual = snapshot(&connection).expect("snapshot");
    drop(connection);

    let other = Snapshot {
        version: actual.version,
        counts: vec![("conversation".to_string(), 1)],
    };
    assert!(verify_copy(&path, &key, &other).is_err());
    assert!(verify_copy(&path, &key, &actual).is_ok());
}
