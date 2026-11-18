//! A database at version 27 gains `approval.operator_reason` when it opens, and a new one
//! starts at 28 with the column (REDI7-CORE). The step is its own migration, never part of
//! an older one: a database already past 27 would never run an older step again.

use excubia_chat::{ApprovalFilter, ChatStore};
use excubia_test_dir::TestDir;

fn version_of(path: &std::path::Path) -> i64 {
    rusqlite::Connection::open(path)
        .expect("raw")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version")
}

#[test]
fn a_database_at_version_27_gains_the_operator_reason_column() {
    let dir = TestDir::new("operator-reason-migration");
    let path = dir.path().join("chat.db");
    drop(ChatStore::open(&path).expect("fresh"));

    // Back to what the host left before this step: no column, version 27.
    let raw = rusqlite::Connection::open(&path).expect("raw");
    raw.execute_batch("ALTER TABLE approval DROP COLUMN operator_reason;")
        .expect("drop");
    raw.pragma_update(None, "user_version", 27)
        .expect("version");
    drop(raw);

    let store = ChatStore::open(&path).expect("reopen");
    store
        .approvals(&ApprovalFilter {
            limit: 10,
            ..ApprovalFilter::default()
        })
        .expect("the approval query still works");
    drop(store);

    assert_eq!(version_of(&path), 28);
    rusqlite::Connection::open(&path)
        .expect("raw again")
        .prepare("SELECT operator_reason FROM approval")
        .expect("column exists");
}

#[test]
fn a_new_database_starts_at_version_28_with_the_column() {
    let dir = TestDir::new("operator-reason-fresh");
    let path = dir.path().join("chat.db");
    drop(ChatStore::open(&path).expect("fresh"));

    assert_eq!(version_of(&path), 28);
    rusqlite::Connection::open(&path)
        .expect("raw")
        .prepare("SELECT operator_reason FROM approval")
        .expect("column exists");
}
