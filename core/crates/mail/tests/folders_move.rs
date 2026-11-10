//! The folder list and the move (F2A) against a mock Graph: the aliases resolved by
//! path, the new id a move returns, the exact body of a move, and the typed refusals.

mod support;

use excubia_core_mail::mover::OTHER_FOLDER;
use excubia_core_mail::{GraphClient, MailError, MoveDest};
use serde_json::json;
use support::mock::{MockResponse, MockTransport};
use support::{in_memory_ledger, ACCESS_TOKEN, ACCOUNT};

const FOLDERS: &str = r#"{"value":[
    {"id":"F-INBOX","displayName":"Recibidos","totalItemCount":40,"unreadItemCount":3},
    {"id":"F-JUNK","displayName":"Correo no deseado","totalItemCount":2,"unreadItemCount":0},
    {"id":"F-CUSTOM","displayName":"Clientes","totalItemCount":9,"unreadItemCount":1},
    {"id":"F-HIST","displayName":"Historial","totalItemCount":0,"unreadItemCount":0}
]}"#;

fn parent(folder_id: &str) -> MockResponse {
    MockResponse::ok(json!({ "parentFolderId": folder_id }).to_string())
}

#[tokio::test]
async fn the_folder_list_names_the_aliases_found_by_their_path() {
    let transport = MockTransport::new(vec![MockResponse::ok(FOLDERS)]).with_fixture_aliases();
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let folders = client.list_folders().await.expect("folders");

    assert_eq!(folders.len(), 4);
    assert_eq!(folders[0].well_known_name.as_deref(), Some("inbox"));
    assert_eq!(folders[0].unread_item_count, 3);
    assert_eq!(folders[1].well_known_name.as_deref(), Some("junkemail"));
    assert_eq!(folders[2].well_known_name, None, "a custom folder has none");
    assert_eq!(folders[2].display_name, "Clientes");
    assert_eq!(
        folders[3].well_known_name, None,
        "a folder that is not an alias is listed without one"
    );
    let calls = transport.calls();
    assert_eq!(calls.len(), 7, "the list, then one GET per alias");
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0].url.contains("/me/mailFolders?$top=50&$select="));
    assert!(!calls[0].url.contains("wellKnownName"));
    assert!(calls[1].url.ends_with("/me/mailFolders/inbox?$select=id"));
    // deleteditems, sentitems, drafts and archive are 404 here: the list still succeeds.
    assert!(calls[6].url.ends_with("/me/mailFolders/archive?$select=id"));
}

#[tokio::test]
async fn an_alias_refused_with_another_error_fails_the_listing() {
    let refusal = r#"{"error":{"code":"ErrorAccessDenied","message":"Acceso denegado"}}"#;
    let transport = MockTransport::new(vec![MockResponse::ok(FOLDERS)])
        .with_fixture_aliases()
        .with_alias("archive", MockResponse::status(403, refusal));
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client.list_folders().await.expect_err("refused");

    match error {
        MailError::Graph { status, code, .. } => {
            assert_eq!(status, 403);
            assert_eq!(code, "ErrorAccessDenied");
        }
        other => panic!("expected a Graph error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_move_returns_the_new_id_and_the_folder_it_left() {
    let transport = MockTransport::new(vec![
        parent("F-INBOX"),
        MockResponse::created(r#"{"id":"AAMk-2"}"#),
    ])
    .with_fixture_aliases();
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let moved = client
        .move_message("AAMk-1", MoveDest::Junk)
        .await
        .expect("moved");

    assert_eq!(moved.id, "AAMk-2");
    assert_eq!(moved.from_folder, "inbox");
    let calls = transport.calls();
    assert_eq!(calls.len(), 8, "the parent, six aliases and the move");
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0]
        .url
        .ends_with("/me/messages/AAMk-1?$select=parentFolderId"));
    assert_eq!(calls[7].method, "POST");
    assert!(calls[7].url.ends_with("/me/messages/AAMk-1/move"));
    assert_eq!(calls[7].text(), r#"{"destinationId":"junkemail"}"#);
}

#[tokio::test]
async fn a_message_in_a_folder_that_is_not_an_alias_comes_from_other() {
    let transport = MockTransport::new(vec![
        parent("F-SUB"),
        MockResponse::created(r#"{"id":"AAMk-2"}"#),
    ])
    .with_fixture_aliases();
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let moved = client
        .move_message("AAMk-1", MoveDest::Inbox)
        .await
        .expect("moved");

    assert_eq!(moved.from_folder, OTHER_FOLDER);
    let calls = transport.calls();
    assert_eq!(
        calls.last().expect("move").text(),
        r#"{"destinationId":"inbox"}"#
    );
}

#[tokio::test]
async fn a_message_that_is_gone_is_not_found() {
    let transport = MockTransport::new(vec![MockResponse::status(404, r#"{"error":{}}"#)]);
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .move_message("AAMk-9", MoveDest::Deleted)
        .await
        .expect_err("not found");

    assert!(matches!(error, MailError::MessageNotFound));
    assert_eq!(transport.calls().len(), 1, "no move is attempted");
}

#[tokio::test]
async fn a_refused_move_is_a_typed_graph_error_without_content() {
    let refusal = r#"{"error":{"code":"ErrorAccessDenied","message":"Acceso denegado"}}"#;
    let transport = MockTransport::new(vec![parent("F-INBOX"), MockResponse::status(403, refusal)])
        .with_fixture_aliases();
    let ledger = in_memory_ledger();
    let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);

    let error = client
        .move_message("AAMk-1", MoveDest::Junk)
        .await
        .expect_err("refused");

    match error {
        MailError::Graph { status, code, .. } => {
            assert_eq!(status, 403);
            assert_eq!(code, "ErrorAccessDenied");
        }
        other => panic!("expected a Graph error, got {other:?}"),
    }
}

#[test]
fn the_destinations_are_the_three_aliases_and_nothing_else() {
    for (alias, dest) in [
        ("junkemail", MoveDest::Junk),
        ("deleteditems", MoveDest::Deleted),
        ("inbox", MoveDest::Inbox),
    ] {
        assert_eq!(MoveDest::from_alias(alias), Some(dest), "{alias}");
        assert_eq!(dest.alias(), alias);
    }
    for other in [
        "archive",
        "drafts",
        "sentitems",
        "",
        "JUNKEMAIL",
        "F-JUNK",
        "permanentDelete",
    ] {
        assert_eq!(MoveDest::from_alias(other), None, "{other}");
    }
}
