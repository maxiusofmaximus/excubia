//! A database made before `headline` existed gets the column when it opens (REDI-ALFRED2
//! shipped it inside an old step, so every database already at 26 failed with
//! «no such column: headline»).

use excubia_chat::{ApprovalFilter, ChatStore};
use excubia_test_dir::TestDir;

#[test]
fn a_database_at_version_26_gains_the_headline_column() {
    let dir = TestDir::new("headline-migration");
    let path = dir.path().join("chat.db");
    drop(ChatStore::open(&path).expect("fresh"));

    // Back to what the 2.3.4 host left: no column, version 26.
    let raw = rusqlite::Connection::open(&path).expect("raw");
    raw.execute_batch("ALTER TABLE approval DROP COLUMN headline;")
        .expect("drop");
    raw.pragma_update(None, "user_version", 26).expect("version");
    drop(raw);

    let store = ChatStore::open(&path).expect("reopen");
    store
        .approvals(&ApprovalFilter {
            limit: 10,
            ..ApprovalFilter::default()
        })
        .expect("the approval query reads the column");
    drop(store);

    let raw = rusqlite::Connection::open(&path).expect("raw again");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert!(version >= 27);
    raw.prepare("SELECT headline FROM approval").expect("column exists");
}
