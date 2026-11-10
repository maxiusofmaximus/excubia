//! The action log and the undo (F2A): the log keeps what moved and nothing of the
//! mail's content, undoing returns a message under its new id, and an action is
//! undone once at most.

mod support;

use excubia_core_mail::{ActionLog, GraphClient, MailActionRecord, MailError, MoveDest, Moved};
use excubia_core_test_dir::TestDir;
use serde_json::Value;
use support::mock::{MockResponse, MockTransport};
use support::{in_memory_ledger, ACCESS_TOKEN, ACCOUNT};

const OTHER_ACCOUNT: &str = "personal";

fn moved(id: &str, from_folder: &str) -> Moved {
    Moved {
        id: id.to_owned(),
        from_folder: from_folder.to_owned(),
    }
}

fn record(account: &str, before: &str, moved: &Moved, to: &str) -> MailActionRecord {
    MailActionRecord::new(account, "mail.move", before, moved, to)
}

#[test]
fn the_log_lists_newest_first_and_marks_one_record_undone() {
    let dir = TestDir::new("mail-actions-log");
    let log = ActionLog::new(&dir.join("accounts"));
    let first = record(ACCOUNT, "AAMk-1", &moved("AAMk-2", "inbox"), "junkemail");
    let other = record(
        OTHER_ACCOUNT,
        "BBMk-1",
        &moved("BBMk-2", "inbox"),
        "junkemail",
    );
    let last = record(ACCOUNT, "AAMk-3", &moved("AAMk-4", "inbox"), "deleteditems");
    for item in [&first, &other, &last] {
        log.append(item).expect("append");
    }

    let recent = log.list_recent(ACCOUNT, 1).expect("recent");
    assert_eq!(recent, vec![last.clone()]);
    let all = log.list_recent(ACCOUNT, 10).expect("recent");
    assert_eq!(all, vec![last.clone(), first.clone()]);
    assert_eq!(
        log.find_by_after_id(ACCOUNT, "AAMk-2").expect("find"),
        Some(first.clone())
    );
    assert_eq!(log.find_by_after_id(ACCOUNT, "BBMk-2").expect("find"), None);

    log.mark_undone(&first.id).expect("mark");

    assert!(
        log.find(ACCOUNT, &first.id)
            .expect("find")
            .expect("kept")
            .undone
    );
    assert!(
        !log.find(ACCOUNT, &last.id)
            .expect("find")
            .expect("kept")
            .undone
    );
    assert_eq!(
        log.find(OTHER_ACCOUNT, &other.id).expect("find"),
        Some(other.clone()),
        "another account's line is untouched"
    );
    assert!(matches!(
        log.mark_undone("no-such-id"),
        Err(MailError::ActionNotFound)
    ));
}

#[test]
fn a_record_holds_only_its_fixed_fields_and_no_content() {
    let entry = record(ACCOUNT, "AAMk-1", &moved("AAMk-2", "inbox"), "junkemail");

    let value: Value = serde_json::to_value(&entry).expect("json");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "account",
            "approval_id",
            "decider_kind",
            "from_folder",
            "id",
            "message_id_after",
            "message_id_before",
            "to_folder",
            "tool",
            "ts",
            "undone",
            "verdict_level",
        ]
    );
    assert!(!value.to_string().contains("ASUNTO-CENTINELA"));
}

#[tokio::test]
async fn undoing_a_move_to_junk_returns_the_message_under_its_new_id() {
    let dir = TestDir::new("mail-actions-undo");
    let log = ActionLog::new(&dir.join("accounts"));
    let move_transport = MockTransport::new(vec![
        MockResponse::ok(r#"{"parentFolderId":"F-INBOX"}"#),
        MockResponse::created(r#"{"id":"AAMk-2"}"#),
    ])
    .with_fixture_aliases();
    let ledger = in_memory_ledger();
    let moving = GraphClient::new(&move_transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let done = moving
        .move_message("AAMk-1", MoveDest::Junk)
        .await
        .expect("moved");
    let original = record(ACCOUNT, "AAMk-1", &done, "junkemail");
    log.append(&original).expect("append");

    let undo_transport = MockTransport::new(vec![
        MockResponse::ok(r#"{"parentFolderId":"F-JUNK"}"#),
        MockResponse::created(r#"{"id":"AAMk-3"}"#),
    ])
    .with_fixture_aliases();
    let undoing = GraphClient::new(&undo_transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    let undo = undoing.undo(&log, &original.id).await.expect("undo");

    assert_eq!(undo.tool, "mail.undo");
    assert_eq!(undo.message_id_before, "AAMk-2");
    assert_eq!(undo.message_id_after, "AAMk-3");
    assert_eq!(undo.from_folder, "junkemail");
    assert_eq!(undo.to_folder, "inbox");
    assert!(!undo.undone);
    let calls = undo_transport.calls();
    let movement = calls.last().expect("the move");
    assert!(movement.url.ends_with("/me/messages/AAMk-2/move"));
    assert_eq!(movement.text(), r#"{"destinationId":"inbox"}"#);
    assert!(
        log.find(ACCOUNT, &original.id)
            .expect("find")
            .expect("kept")
            .undone
    );
    assert_eq!(
        log.find_by_after_id(ACCOUNT, "AAMk-3").expect("find"),
        Some(undo),
        "the undo is recorded as a move of its own"
    );

    let again = MockTransport::new(Vec::new());
    let twice = GraphClient::new(&again, ACCESS_TOKEN, &ledger, ACCOUNT);
    assert!(matches!(
        twice.undo(&log, &original.id).await,
        Err(MailError::ActionUndone)
    ));
    assert!(again.calls().is_empty(), "a second undo asks Graph nothing");
}

#[tokio::test]
async fn a_move_from_a_folder_undo_cannot_return_to_is_refused_clearly() {
    let dir = TestDir::new("mail-actions-other");
    let log = ActionLog::new(&dir.join("accounts"));
    let stranded = record(ACCOUNT, "AAMk-1", &moved("AAMk-2", "other"), "junkemail");
    log.append(&stranded).expect("append");
    let transport = MockTransport::new(Vec::new());
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .undo(&log, &stranded.id)
        .await
        .expect_err("not undoable");

    assert!(matches!(error, MailError::NotUndoable));
    assert!(error.to_string().contains("cannot be undone automatically"));
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn another_accounts_action_is_not_found_here() {
    let dir = TestDir::new("mail-actions-account");
    let log = ActionLog::new(&dir.join("accounts"));
    let foreign = record(
        OTHER_ACCOUNT,
        "BBMk-1",
        &moved("BBMk-2", "inbox"),
        "junkemail",
    );
    log.append(&foreign).expect("append");
    let transport = MockTransport::new(Vec::new());
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client.undo(&log, &foreign.id).await.expect_err("not found");

    assert!(matches!(error, MailError::ActionNotFound));
}
