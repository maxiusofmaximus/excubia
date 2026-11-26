//! A database made before `category` existed gets the column when it opens, and its old
//! rows keep NULL, which «always in this task» never covers.

use excubia_chat::{ApprovalFilter, ChatStore};
use excubia_test_dir::TestDir;

#[test]
fn a_database_at_version_31_gains_the_category_column() {
    let dir = TestDir::new("category-migration");
    let path = dir.path().join("chat.db");
    drop(ChatStore::open(&path).expect("fresh"));

    // Back to what the previous host left: no column, version 31.
    let raw = rusqlite::Connection::open(&path).expect("raw");
    raw.execute_batch("ALTER TABLE approval DROP COLUMN category;")
        .expect("drop");
    raw.pragma_update(None, "user_version", 31).expect("version");
    drop(raw);

    let store = ChatStore::open(&path).expect("reopen");
    store
        .approvals(&ApprovalFilter {
            limit: 10,
            ..ApprovalFilter::default()
        })
        .expect("the approval query still works");
    drop(store);

    let raw = rusqlite::Connection::open(&path).expect("raw again");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert!(version >= 32);
    raw.prepare("SELECT category FROM approval").expect("column exists");
}
